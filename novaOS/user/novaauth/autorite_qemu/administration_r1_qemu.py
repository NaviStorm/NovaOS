"""R1 invité : consultations privées, portée contrôlée à chaque demande.

Cette campagne ne qualifie pas les mutations ni les clôtures R2.
Le lanceur du banc doit détenir le verrou commun QEMU.
"""
import argparse, copy, json, os, re
from contextlib import ExitStack
from pathlib import Path
from recuperation_qemu import fixture, run, SERVER, DOMAIN
from exec_deux_vm import route, signature
from reprise import Refus
import admission_exec as execs

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('arch',choices=['aarch64','x86_64'])
    p.add_argument('sector',type=int,choices=[512,4096])
    p.add_argument('--etendu',action='store_true')
    for name in ('iso','registre','genese','cles','decision','secrets','sortie','detenteur','capture','triplet'):
        p.add_argument('--'+name,type=lambda v:Path(v).resolve(),required=True)
    a=p.parse_args();a.mode='nominal';a.vue=a.activation=a.exec_admission=True
    a.vfs=a.mutation=False;a.lecture_privee='autorisee'
    # Les consultations fraîches continuent pendant le témoin d'expiration
    # de 60 s. Ce budget hôte ne change ni les délais TLS invités, ni les quotas.
    # Garder la borne historique de 128 pour tous les autres bancs.
    # Quatre authentifications explicites supplémentaires entre groupes ont
    # épuisé 256 échanges après la sortie du dernier enfant (trace conservée).
    # Réserver 16 échanges hôtes séquentiels par nouveau groupe, sans changer
    # le nombre de connexions simultanées ni aucun budget invité.
    a.transport_connection_limit=(256+4*16) if a.etendu else 128
    a.legacy_disk=a.genese.with_name('autorite.raw')
    a.sortie.mkdir(parents=True,exist_ok=False);a.secrets.mkdir(parents=True,exist_ok=False,mode=0o700)
    root=a.sortie
    report={'succes':False,'portee':'R1 consultations QEMU ; aucune mutation R2','architecture':a.arch,'secteur':a.sector,'commandes':[]}
    with ExitStack() as stack:
        env=fixture(a,stack);dispatcher,holders,admin=env
        api=execs.Admissions(dispatcher.authority.authority);execs.initialize(api.path)
        old=route(api);api.enroll(SERVER,DOMAIN,old,signature(admin,'enroll',old))
        api.begin_withdrawal(SERVER,DOMAIN,old,signature(admin,'withdraw',old));api.drain(SERVER,DOMAIN,old)
        def scenario(serial,control,disk,out):
            serial.expect(rb'NAEXEC registre pret puis connexion du vrai nsh',100)
            if b'nsh> ' not in serial.data:serial.expect(rb'nsh> ',20)
            rc,raw=serial.command('true')
            if rc:raise Refus('authentification initiale refusée')
            oracle=a.genese.with_name('oracle-self.bin').read_bytes()
            refused=(5).to_bytes(8,'little')+bytes(56)
            for line,status,expected in [('admin-sonde --self',0,oracle),('admin-sonde --user bob',5,refused),('admin-sonde --self',0,oracle)]:
                rc,raw=serial.command(line)
                (out/('commande-'+str(len(report['commandes']))+'.bin')).write_bytes(raw)
                streams=re.findall(rb'NXFLUX pid=([0-9]+) fd=([12]) octets=([0-9]+) prefixe=([0-9a-f]*)',raw)
                actual={int(fd):(int(n),bytes.fromhex(h.decode())) for _,fd,n,h in streams}
                row={'commande':line,'statut':rc,'attendu':status,'flux':[(int(pid),int(fd),int(n),h.decode()) for pid,fd,n,h in streams]}
                report['commandes'].append(row)
                if rc!=status or actual!={1:(len(expected),expected),2:(0,b'')}:
                    raise Refus('résultat ou octets R1 incorrects : '+repr(row))
            if a.etendu:
                report['refus_cibles']=[]
                report['authentifications_intermediaires']=[]
                def reauthenticate(label):
                    rc,trace=serial.command('session reauthentifier')
                    (out/('authentification-'+label+'.bin')).write_bytes(trace)
                    report['authentifications_intermediaires'].append({'avant':label,'statut':rc})
                    if rc:raise Refus('nouvelle authentification explicite refusée : '+label)
                for option,status,expected in [
                    ('--denied-same-channel',0,bytes(8)+(5).to_bytes(8,'little')+bytes(48)),
                    ('--replay',3,(3).to_bytes(8,'little')+bytes(56)),
                    ('--invalid-frame',3,(3).to_bytes(8,'little')+bytes(56)),
                    ('--forged-context',3,(3).to_bytes(8,'little')+bytes(56)),
                    ('--forged-meta',5,(5).to_bytes(8,'little')+bytes(56))]:
                    # Groupes indépendants, aucune relance après mutation ni
                    # prolongation d'un bail au milieu de la conversation.
                    if option in ('--denied-same-channel','--invalid-frame','--forged-meta'):
                        reauthenticate(option[2:])
                    rc,raw=serial.command('admin-sonde '+option)
                    streams=re.findall(rb'NXFLUX pid=([0-9]+) fd=([12]) octets=([0-9]+) prefixe=([0-9a-f]*)',raw)
                    actual={int(fd):(int(n),bytes.fromhex(h.decode())) for _,fd,n,h in streams}
                    row={'option':option,'statut':rc,'attendu':status,'flux':[(int(pid),int(fd),int(n),h.decode()) for pid,fd,n,h in streams]}
                    report['refus_cibles'].append(row)
                    if rc!=status or actual!={1:(64,expected),2:(0,b'')}:raise Refus('refus R1 non conforme : '+repr(row))
                reauthenticate('perte-et-concurrence')
                rc,raw=serial.command('admin-sonde --lost-reply')
                expected=(22).to_bytes(8,'little')+bytes(56)
                streams=re.findall(rb'NXFLUX pid=([0-9]+) fd=([12]) octets=([0-9]+) prefixe=([0-9a-f]*)',raw)
                actual={int(fd):(int(n),bytes.fromhex(h.decode())) for _,fd,n,h in streams}
                report['reponse_partielle_perdue']={'statut':rc,'flux':[(int(pid),int(fd),int(n),h.decode()) for pid,fd,n,h in streams]}
                if rc!=22 or actual!={1:(64,expected),2:(0,b'')}:raise Refus('réponse perdue non qualifiée')
                rc,raw=serial.command('admin-sonde --concurrent-writer | admin-sonde --concurrent-reader')
                pair=re.search(rb'NXPIPE producteur=([0-9]+) statut=([0-9]+) lecteur=([0-9]+) statut=([0-9]+)',raw)
                if not pair:raise Refus('statuts des deux ELF absents')
                producer,pr,reader,rr=map(int,pair.groups())
                streams=re.findall(rb'NXFLUX pid=([0-9]+) fd=([12]) octets=([0-9]+) prefixe=([0-9a-f]*)',raw)
                actual={(int(pid),int(fd)):(int(n),bytes.fromhex(h.decode())) for pid,fd,n,h in streams}
                report['concurrence']={'producteur':producer,'lecteur':reader,'statut_producteur':pr,'statut_lecteur':rr,'statut_pipe':rc}
                if producer==reader or rc or pr or rr or actual!={(producer,2):(0,b''),(reader,1):(64,oracle),(reader,2):(0,b'')}:
                    raise Refus('conversations concurrentes divergentes : '+repr(report['concurrence']))
                # Le lecteur ne draine les neuf trames de synchronisation qu'après l'ACK
                # de sa première trame. Le producteur conserve alors sa requête
                # partielle, donc deux conversations ont réellement coexisté.
                rc,_=serial.command('session reauthentifier')
                if rc:raise Refus('réauthentification explicite avant témoin expiration')
                start=serial.offset
                serial.submit('admin-sonde --expire')
                child=serial.expect(rb'NAEXEC enfant owner=([0-9]+) child=([0-9]+)\r?\n',30)
                pid=int(child[2]);exit=serial.expect(fr'PROC pid={pid} niveau=[^ ]+ pc=0x[0-9a-f]+ sortie=([0-9]+)\r?\n'.encode(),100)
                if int(exit[1])!=10:raise Refus('ancien handle non révoqué : '+exit[0].decode())
                serial.expect(rb'nsh> ',20)
                report['expiration']={'pid':pid,'statut_natif':10,'seconde_utilisation_refusee':True}
                (out/'expiration.bin').write_bytes(serial.data[start:serial.offset])
                rc,_=serial.command('session reauthentifier')
                if rc:raise Refus('réauthentification explicite après expiration')
                rc,raw=serial.command('admin-sonde --self')
                captured=re.search(rb'NXFLUX pid=([0-9]+) fd=1 octets=64 prefixe=([0-9a-f]+)',raw)
                if rc or not captured or bytes.fromhex(captured[2].decode())!=oracle or int(captured[1])==pid:
                    raise Refus('nouvelle session non fonctionnelle après révocation')
                report['nouvelle_session']={'statut':rc,'pid':int(captured[1]),'octets':captured[2].decode()}
            report['succes']=True
            return report
        try:
            case=copy.copy(a);case.sortie=root/'invite'
            return run(case,env,scenario=scenario)
        finally:
            (root/'r1-resultats.json').write_text(json.dumps(report,indent=2)+'\n')
if __name__=='__main__':raise SystemExit(main())
