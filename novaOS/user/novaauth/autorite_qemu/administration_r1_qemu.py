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
    for name in ('iso','registre','genese','cles','decision','secrets','sortie','detenteur','capture','triplet'):
        p.add_argument('--'+name,type=lambda v:Path(v).resolve(),required=True)
    a=p.parse_args();a.mode='nominal';a.vue=a.activation=a.exec_admission=True
    a.vfs=a.mutation=False;a.lecture_privee='autorisee'
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
            report['succes']=True
            return report
        try:
            case=copy.copy(a);case.sortie=root/'invite'
            return run(case,env,scenario=scenario)
        finally:
            (root/'r1-resultats.json').write_text(json.dumps(report,indent=2)+'\n')
if __name__=='__main__':main()
