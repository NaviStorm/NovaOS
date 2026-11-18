"""Banc de tickets invités ; une autorité durable hors des disques des VM.

Le mode de diagnostic initial ne revendique pas la qualification à deux VM.
"""
from contextlib import ExitStack,closing
from pathlib import Path
import argparse,copy,json,os
from recuperation_qemu import fixture,run,SERVER,DOMAIN
from reprise import open_database,Refus
import admission_exec as execs

def route(api):
    with closing(open_database(api.path)) as db:
        return db.execute('SELECT instance FROM scope WHERE server=? AND domain=?',(SERVER,DOMAIN)).fetchone()[0]

def signature(administrator,kind,instance):
    return administrator.call('exec-route',kind,SERVER,DOMAIN,instance,b'')

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('arch',choices=['aarch64','x86_64']);p.add_argument('sector',type=int,choices=[512,4096])
    for name in ('iso','registre','genese','cles','decision','secrets','sortie','detenteur','capture','triplet'):
        p.add_argument('--'+name,type=lambda v:Path(v).resolve(),required=True)
    p.add_argument('--diagnostic',action='store_true',required=True)
    a=p.parse_args();a.mode='nominal';a.vue=a.activation=a.exec_admission=True
    a.vfs=a.mutation=False;a.lecture_privee='autorisee'
    adapter=Path(os.environ['NOVA_ADAPTATEUR']).resolve(strict=True)
    if not adapter.is_file() or not os.access(adapter,os.X_OK):
        raise Refus('adaptateur natif absent ou non exécutable')
    a.sortie.mkdir(parents=True,exist_ok=False);a.secrets.mkdir(parents=True,exist_ok=False,mode=0o700)
    root=a.sortie
    with ExitStack() as stack:
        environment=fixture(a,stack);dispatcher,holders,admin=environment
        api=execs.Admissions(dispatcher.authority.authority)
        execs.initialize(api.path)
        old=route(api)
        api.enroll(SERVER,DOMAIN,old,signature(admin,'enroll',old))
        api.begin_withdrawal(SERVER,DOMAIN,old,signature(admin,'withdraw',old))
        api.drain(SERVER,DOMAIN,old)
        # La route synthétique initiale n'a jamais porté de processus invité.
        def scenario(serial,control,disk,out):
            serial.expect(rb'NAEXEC registre pret puis connexion du vrai nsh',100)
            if b'nsh> ' not in serial.data:
                serial.expect(rb'nsh> ',20)
            status,output=serial.command('true')
            if status:raise Refus(f'authentification shell : {status}')
            first=route(api)
            status,output=serial.command('cut -f1,3 /travail/donnees.tsv')
            (out/'commande-cut.bin').write_bytes(output)
            with closing(open_database(api.path)) as db:
                rows=db.execute('SELECT hex(op),state,hex(child),sequence,terminal_sequence FROM exec_admissions').fetchall()
            return {'succes':status==0 and len(rows)==1 and rows[0][1]==1,'statut_cut':status,'incarnation':first.hex(),'admissions':rows,'deux_VM':False}
        case=copy.copy(a);case.sortie=root/'diagnostic-A'
        return run(case,environment,scenario=scenario)
if __name__=='__main__':raise SystemExit(main())
