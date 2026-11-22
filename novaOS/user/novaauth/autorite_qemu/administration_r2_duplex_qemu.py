"""Diagnostic ciblé R2 à deux sessions. Ne qualifie pas encore les mutations.

Le lanceur doit posséder le verrou QEMU commun. Aucun droit du collecteur n'est
remis aux invités. Les sorties Bob devront être vérifiées par la sonde privée.
"""
import argparse, copy, json, re, time
from contextlib import ExitStack
from pathlib import Path
from recuperation_qemu import fixture, run, SERVER, DOMAIN
from exec_deux_vm import route, signature
from reprise import Refus, transaction
import admission_exec as execs
from administration_epochs import Epochs, initialize


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('arch',choices=['aarch64','x86_64'])
    parser.add_argument('sector',type=int,choices=[512,4096])
    parser.add_argument('--fermer',action='store_true',help='Vérifier aussi la fermeture supervisée des deux sessions')
    for name in ('iso','registre','genese','cles','decision','secrets','sortie','detenteur','capture','triplet','kernel'):
        parser.add_argument('--'+name,type=lambda v:Path(v).resolve(),required=True)
    a=parser.parse_args();a.mode='nominal';a.vue=a.activation=a.exec_admission=a.administration_r2=True
    a.collecteur_r2=True
    a.vfs=a.mutation=False;a.lecture_privee='autorisee';a.transport_connection_limit=128
    a.legacy_disk=a.genese.with_name('autorite.raw')
    a.sortie.mkdir(parents=True,exist_ok=False);a.secrets.mkdir(parents=True,exist_ok=False,mode=0o700)
    root=a.sortie
    report={'succes':False,'portee':'diagnostic duplex et sorties ; mutations et révocation non qualifiées','architecture':a.arch,'secteur':a.sector,'commandes':[]}
    raw=a.triplet.read_bytes();size=int.from_bytes(raw[152:156],'big')
    users=json.loads(raw[156:156+size]);subjects=tuple((bytes(u['id']),u['authentication']['epoch']) for u in users if u['status']=='Active')
    with ExitStack() as stack:
        env=fixture(a,stack);dispatcher,holders,admin=env
        api=execs.Admissions(dispatcher.authority.authority);execs.initialize(api.path);initialize(api.path)
        admin.call('r2-subjects',subjects)
        old=route(api);api.enroll(SERVER,DOMAIN,old,signature(admin,'enroll',old))
        api.begin_withdrawal(SERVER,DOMAIN,old,signature(admin,'withdraw',old));api.drain(SERVER,DOMAIN,old)
        enrolled=False
        def rpc_hook(phase,operation,body,result,qmp,disk):
            nonlocal enrolled
            if phase=='before' and operation==40 and not enrolled:
                with transaction(api.path) as db:
                    state=api.authority._current(db,SERVER,DOMAIN)
                proof=admin.call('r2-enroll',state)
                receipt,sig=Epochs(api.authority).enroll(state,subjects,proof)
                (root/'enrolement-epoques.bin').write_bytes(receipt+sig)
                enrolled=True
        def scenario(serial,control,disk,out):
            from administration_collector_qemu import Observer
            observer=Observer(control,a.arch,a.kernel,out)
            stack.callback(observer.close)
            serial.expect(rb'NAEXEC registre pret puis connexion du vrai nsh',100)
            if b'nsh> ' not in serial.data:serial.expect(rb'nsh> ',20)
            rc,raw=serial.command('true')
            if rc:raise Refus('authentification Alice refusée')
            marker=re.escape(b'NAR2 Bob authentifie route distincte')
            # La session de Bob est indépendante et peut avoir répondu pendant
            # la commande d'Alice. Reprendre cette trace unique, sans renvoyer
            # aucune commande ni demander une seconde authentification.
            known=re.search(marker,serial.data)
            if known:serial.offset=known.end()
            else:serial.expect(marker,40)
            level=b'EL0' if a.arch=='aarch64' else b'Ring3'
            # Observation hôte bornée : huit échanges du même ELF. Ne pas
            # interrompre l'invité avant son résultat natif, notamment en P9/4096.
            # Aucune échéance TLS, IPC ou de session invitée n'est modifiée.
            serial.expect(re.escape(b'NAR2 Bob refus serveur et substitutions verifies'),80)
            serial.expect(re.escape(b'NAR2 Bob fermeture acquittee'),40)
            if b'NAR2 Bob paquets Alice et non autorise refuses statut=5 aucun enfant' not in serial.data:
                raise Refus('refus des installations non attesté dans l’invité')
            # La commande métier et la lecture de $? sont dans la même ligne.
            # Le bail n'est ni étendu ni renouvelé ; les deux statuts natifs et
            # les octets restent contrôlés indépendamment ci-dessous.
            serial.offset=len(serial.data)
            start=serial.offset
            serial.submit('alice-seulement --self; echo NXR_$?__FIN'+('; exit' if a.fermer else ''))
            status=serial.expect(rb'NXR_(\d+)__FIN\r?\n')
            if not a.fermer:serial.expect(rb'nsh> ')
            rc,raw=int(status[1]),bytes(serial.data[start:serial.offset])
            (out/'alice-consultation.bin').write_bytes(raw)
            oracle=a.genese.with_name('oracle-self.bin').read_bytes()
            streams=re.findall(rb'NXFLUX pid=([0-9]+) fd=([12]) octets=([0-9]+) prefixe=([0-9a-f]*)',raw)
            actual={int(fd):(int(n),bytes.fromhex(h.decode())) for _,fd,n,h in streams}
            report['commandes'].append({'commande':'alice-seulement --self','statut':rc,'flux':[(int(pid),int(fd),int(n),h.decode()) for pid,fd,n,h in streams]})
            if rc or actual!={1:(len(oracle),oracle),2:(0,b'')}:raise Refus('consultation Alice non conforme')
            rows=observer.outputs()
            streams=[]
            for binding,closed,data in rows:
                if not closed:raise Refus('flux non fermé')
                path=out/f'flux-{binding.shell}-{binding.child}-{binding.fd}.bin';path.write_bytes(data)
                streams.append({'shell':binding.shell,'pid':binding.child,'fd':binding.fd,'octets':len(data),'fichier':path.name})
            by_shell={}
            for binding,closed,data in rows:
                by_shell.setdefault(binding.shell,{}).setdefault(binding.child,{})[binding.fd]=data
            bob_oracle=a.genese.with_name('oracle-bob.bin').read_bytes()
            codes=dict((int(p),int(c)) for p,c in re.findall(rb'PROC pid=(\d+) niveau='+level+rb' pc=0x[0-9a-f]+ sortie=(\d+)',serial.data))
            denied=(5).to_bytes(8,'little')+bytes(56)
            malformed=(3).to_bytes(8,'little')+bytes(56)
            expected_bob=[(0,b''.join([bob_oracle,denied,denied,bob_oracle,denied,malformed,malformed,bob_oracle]))]
            witnessed=[]
            for shell,children in by_shell.items():
                actual=[(codes.get(pid),fds.get(1),fds.get(2)) for pid,fds in sorted(children.items())]
                if actual==[(0,oracle,b'')]:witnessed.append('Alice')
                elif actual==[(code,data,b'') for code,data in expected_bob]:
                    witnessed.append('Bob')
                    report['terminaisons_bob']=[{'pid':pid,'attendu':0,'observe':codes.get(pid)} for pid in sorted(children)]
                else:raise Refus('sorties ou statuts natifs non conformes : '+str(shell))
            if sorted(witnessed)!=['Alice','Bob']:raise Refus('sessions manquantes ou confondues')
            report['refus_bob']=['annuaire Alice','requête ajout administratif directe','contexte forgé','métadonnées forgées','rejeu de trame']
            report['installations_refusees_bob']=['alice-seulement','paquet-non-autorise']
            report['statuts_requetes_bob']=[0,5,5,0,5,3,3,0]
            report['sorties_exclusivement_hote']=streams
            report['enrolement_époques']=enrolled
            report['bob_consultation_statut_zero']=True
            report['fermeture_bob_avant_operation_alice']=True
            report['statut_shell_dans_meme_ligne']=True
            observer.close()
            if a.fermer:
                serial.expect(rb'BLOC bilan mode=41 service=Exit\(0\) bloc=Exit\(0\) pilote=Exit\(0\) client=Exit\(0\) retrait=Ok\(\(\)\)',40)
                for pid in by_shell:
                    if not re.search(rb'NAEXEC restitue pid='+str(pid).encode()+rb' roots=0 handles=0 waits=0 pages=0 pile=0 threads=0',serial.data):
                        raise Refus('ressources de session non restituées')
                if not re.search(rb'endpoints=0 nodes=0 espaces=0 cadres_ipc=0',serial.data):
                    raise Refus('arrêt sans drainage global complet')
                report['fermeture_deux_sessions_et_arret_invite']=True
            report['succes']=True
            return report
        try:
            case=copy.copy(a);case.sortie=root/'invite'
            return run(case,env,scenario=scenario,rpc_hook=rpc_hook)
        finally:
            (root/'r2-duplex-resultats.json').write_text(json.dumps(report,indent=2)+'\n')
if __name__=='__main__':raise SystemExit(main())
