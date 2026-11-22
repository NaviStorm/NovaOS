"""Observation QEMU privée : lectures GDB seulement, aucun handle aux sessions."""
import hashlib, json, os, socket, threading, time
from types import SimpleNamespace
from exec_vm_debug import Debug,symbol
from administration_outputs import Collector,read_stopped,Refus

class Observer:
    def __init__(self,control,arch,kernel,out):
        self.routes=[];self.records=[];self.error=None;self.collector=None;self.last_pc=None
        self.done=threading.Event();self.ready=threading.Event();self.guard=threading.Lock()
        self.addresses={symbol(kernel,'nova_r2_session_observe'):'session',symbol(kernel,'nova_r2_output_observe'):'output'}
        self.control,self.arch,self.out=control,arch,out
        self.debug=None;self.thread=threading.Thread(target=self.worker,name='collecteur-sorties-R2')
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
                for at in self.addresses:d.breakpoint(at)
                d.resume();self.ready.set()
                while not self.done.is_set():
                    # Une période sans sortie n'est pas une panne de l'invité.
                    # Le délai global du banc borne la collecte ; cette attente
                    # n'altère ni les horloges ni les échéances NovaAuth.
                    try:stop=d.stopped()
                    except TimeoutError:continue
                    pc=d.register(32 if self.arch=='aarch64' else 16)
                    self.last_pc=pc
                    kind=self.addresses.get(pc)
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
                    answer=d.command('s')
                    diagnostic(pas=answer,pc=d.register(32 if self.arch=='aarch64' else 16),cpu=d.command('qC'))
                    if not answer.startswith(('T05','S05')):raise Refus('pas de sonde inattendu')
                    d.breakpoint(pc);d.resume()
        except Exception as e:
            if not self.done.is_set():self.error=type(e).__name__+': '+str(e)
            self.ready.set()
        finally:
            (self.out/'collecteur-diagnostic.json').write_text(json.dumps({
                'erreur':self.error,'dernier_pc':self.last_pc,
                'sessions':len(self.routes),'evenements':len(self.records),
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
