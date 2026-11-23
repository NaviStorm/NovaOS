"""Observation QEMU privée : lectures GDB seulement, aucun handle aux sessions."""
import hashlib, json, os, re, socket, threading, time
from types import SimpleNamespace
from exec_vm_debug import Debug,symbol
from administration_outputs import Collector,read_stopped,Refus,WIRE

def stopped_thread(debug, stop):
    match=re.fullmatch(r'T05thread:([0-9a-fA-F]+);',stop)
    if match is None or int(match[1],16)==0:raise Refus('CPU arrêté non identifié')
    thread=match[1]
    if debug.command('Hg'+thread)!='OK':raise Refus('sélection du CPU arrêté refusée')
    return thread

def advance_probe(debug, arch, pc, thread, diagnostic):
    # Un arrêt GDB peut rendre le même PC sans avoir exécuté l'instruction.
    # Trois demandes au maximum, même CPU et pile inchangée tant que PC stagne.
    # Ne jamais déplacer PC, réinterpréter l'événement ou accepter un pas absent.
    sp_index = 31 if arch == 'aarch64' else 7
    pc_index = 32 if arch == 'aarch64' else 16
    before_sp = debug.register(sp_index)
    for attempt in range(1, 4):
        answer = debug.command('vCont;s:' + thread)
        after_thread = stopped_thread(debug, answer)
        after_pc = debug.register(pc_index)
        diagnostic(pas=answer, pc=after_pc, cpu=debug.command('qC'), cpu_cible=thread, tentative=attempt)
        if int(after_thread, 16) != int(thread, 16):
            raise Refus('pas ciblé sur un autre CPU')
        if after_pc != pc:
            return
        if debug.register(sp_index) != before_sp:
            raise Refus('PC inchangé avec pile modifiée')
    raise Refus('pas ciblé sans progression après trois demandes')


def probe_kind(addresses, pc, size):
    # LLVM peut fusionner les deux fonctions d'observation identiques. Le PC
    # doit toujours appartenir aux sondes ; la taille ne suffit pas à elle seule.
    expected={'session':80,'output':WIRE}
    matches=[kind for kind, address in addresses.items()
             if address==pc and expected[kind]==size]
    if len(matches)!=1:raise Refus('adresse ou format de sonde R2 invalide')
    return matches[0]


class Observer:
    def __init__(self,control,arch,kernel,out):
        self.routes=[];self.records=[];self.error=None;self.collector=None;self.last_pc=None
        self.done=threading.Event();self.ready=threading.Event();self.guard=threading.Lock()
        self.addresses={kind:symbol(kernel,'nova_r2_'+kind+'_observe') for kind in ('session','output')}
        self.control,self.arch,self.out=control,arch,out
        self.debug=None;self.thread=threading.Thread(target=self.worker,name='collecteur-sorties-R2')
        self.expected_successor=None;self.successor_used=False
        self.thread.start()
        if not self.ready.wait(15):
            self.close()
            raise Refus('initialisation collecteur expirée')
        try:self.check()
        except Exception:
            self.close()
            raise
    def check(self):
        if self.error:raise Refus('collecteur : '+self.error)
    def expect_successor(self, shell):
        self.check()
        with self.guard:
            routes=[r for r in self.routes if r[0]==shell]
            if self.successor_used or self.expected_successor is not None or len(routes)!=1:
                raise Refus('succession non ciblée ou déjà consommée')
            if any(not s.closed for s in self.collector.streams.values() if s.binding.shell==shell):
                raise Refus('sorties non drainées avant réauthentification')
            self.expected_successor=routes[0]
    def worker(self):
        rd,wr=os.pipe()
        try:
            # Debug.packet ne lit jamais stdout de la VM : le banc série conserve
            # son lecteur exclusif. Ce pipe reste vide et sert de descripteur neutre.
            with os.fdopen(rd,'rb',buffering=0) as dummy:
                serial=SimpleNamespace(vm=SimpleNamespace(stdout=dummy))
                deadline=time.monotonic()+10
                while not (self.control/'gdb.sock').exists():
                    if time.monotonic()>deadline:raise Refus('socket GDB absente')
                    time.sleep(.01)
                d=self.debug=Debug(self.control/'gdb.sock',serial,self.arch)
                d.command('?')
                if ';s' not in d.command('vCont?'):raise Refus('pas ciblé GDB absent')
                for at in set(self.addresses.values()):d.breakpoint(at)
                d.resume();self.ready.set()
                while not self.done.is_set():
                    # Une période sans sortie n'est pas une panne de l'invité.
                    # Le délai global du banc borne la collecte ; cette attente
                    # n'altère ni les horloges ni les échéances NovaAuth.
                    try:stop=d.stopped()
                    except TimeoutError:continue
                    thread=stopped_thread(d,stop)
                    pc=d.register(32 if self.arch=='aarch64' else 16)
                    self.last_pc=pc
                    size=d.register(1 if self.arch=='aarch64' else 4)
                    kind=probe_kind(self.addresses,pc,size)
                    def diagnostic(**fields):
                        # CPU/PC et empreinte seulement : aucun contenu de
                        # session, pointeur de données ou mémoire arbitraire.
                        with (self.out/'collecteur-arrets.jsonl').open('a') as f:
                            f.write(json.dumps(fields)+'\n')
                    diagnostic(arret=stop,pc=pc,cpu=d.command('qC'),sonde=kind)
                    if kind is None:raise Refus('arrêt hors sondes R2')
                    with self.guard:
                        if kind=='session':
                            ptr=d.register(0 if self.arch=='aarch64' else 5)
                            size=d.register(1 if self.arch=='aarch64' else 4)
                            if size!=80 or not ptr:raise Refus('session de sonde invalide')
                            raw=bytes.fromhex(d.command(f'm{ptr:x},{size:x}'))
                            if len(raw)!=80 or raw[:8]!=b'NXR2SES1':raise Refus('session non canonique')
                            diagnostic(session_sha256=hashlib.sha256(raw).hexdigest())
                            route=(int.from_bytes(raw[8:16],'little'),raw[16:48],raw[48:80])
                            if self.expected_successor is not None:
                                previous=self.expected_successor
                                self.collector.replace_session(previous,route)
                                self.routes[self.routes.index(previous)]=route
                                self.expected_successor=None;self.successor_used=True
                            else:
                                if route in self.routes or len(self.routes)>=2:raise Refus('route réutilisée ou hors borne')
                                self.routes.append(route)
                                self.collector=Collector(self.routes)
                                for record in self.records:self.collector.accept(record)
                            with (self.out/'sessions-noyau.bin').open('ab') as f:f.write(raw)
                        else:
                            if self.collector is None or len(self.records)>=2048:raise Refus('sortie sans route ou quota dépassé')
                            raw=read_stopped(d);self.collector.accept(raw);self.records.append(raw)
                            with (self.out/'sorties-noyau.bin').open('ab') as f:f.write(raw)
                    d.breakpoint(pc,False)
                    # Le pas historique non ciblé pouvait reprendre un autre
                    # vCPU. Ne jamais dédupliquer l'événement ni déplacer PC.
                    advance_probe(d,self.arch,pc,thread,diagnostic)
                    d.breakpoint(pc);d.send('vCont;c')
        except Exception as e:
            if not self.done.is_set():self.error=type(e).__name__+': '+str(e)
            self.ready.set()
        finally:
            (self.out/'collecteur-diagnostic.json').write_text(json.dumps({
                'erreur':self.error,'dernier_pc':self.last_pc,
                'sessions':len(self.routes),'evenements':len(self.records),
                'succession_explicite':self.successor_used,
                'arret_demande':self.done.is_set()},indent=2)+'\n')
            os.close(wr)
            if self.debug:self.debug.close()
    def outputs(self):
        self.check()
        with self.guard:
            if self.collector is None:raise Refus('aucune sortie collectée')
            return [(s.binding,s.closed,bytes(s.data)) for s in self.collector.streams.values()]
    def close(self):
        self.done.set()
        if self.debug:
            try:self.debug.s.shutdown(socket.SHUT_RDWR)
            except OSError:pass
        self.thread.join(5)
        if self.thread.is_alive():raise Refus('collecteur non drainé')
