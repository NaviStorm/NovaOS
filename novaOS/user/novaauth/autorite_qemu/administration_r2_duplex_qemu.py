"""Diagnostic ciblé R2 à deux sessions. Ne qualifie pas encore les mutations.

Le lanceur doit posséder le verrou QEMU commun. Aucun droit du collecteur n'est
remis aux invités. Les sorties Bob devront être vérifiées par la sonde privée.
"""
import argparse, copy, json, re
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
            for label in [b'NAR2 Bob authentifie route distincte',b'NAR2 Bob consultation propre terminee']:
                if label not in serial.data:serial.expect(re.escape(label),40)
            rc,raw=serial.command('admin-sonde --self')
            (out/'alice-consultation.bin').write_bytes(raw)
            oracle=a.genese.with_name('oracle-self.bin').read_bytes()
            streams=re.findall(rb'NXFLUX pid=([0-9]+) fd=([12]) octets=([0-9]+) prefixe=([0-9a-f]*)',raw)
            actual={int(fd):(int(n),bytes.fromhex(h.decode())) for _,fd,n,h in streams}
            report['commandes'].append({'commande':'admin-sonde --self','statut':rc,'flux':[(int(pid),int(fd),int(n),h.decode()) for pid,fd,n,h in streams]})
            if rc or actual!={1:(len(oracle),oracle),2:(0,b'')}:raise Refus('consultation Alice non conforme')
            rows=observer.outputs()
            streams=[]
            for binding,closed,data in rows:
                if not closed:raise Refus('flux non fermé')
                path=out/f'flux-{binding.shell}-{binding.child}-{binding.fd}.bin';path.write_bytes(data)
                streams.append({'shell':binding.shell,'pid':binding.child,'fd':binding.fd,'octets':len(data),'fichier':path.name})
            by_shell={}
            for binding,closed,data in rows:by_shell.setdefault(binding.shell,{})[binding.fd]=data
            bob_oracle=a.genese.with_name('oracle-bob.bin').read_bytes()
            if len(by_shell)!=2 or sorted(by_shell.values(),key=lambda v:v[1])!=sorted([{1:oracle,2:b''},{1:bob_oracle,2:b''}],key=lambda v:v[1]):
                raise Refus('sorties binaires Alice/Bob non conformes')
            report['sorties_exclusivement_hote']=streams
            report['enrolement_époques']=enrolled
            report['bob_consultation_statut_zero']=True
            report['succes']=True
            return report
        try:
            case=copy.copy(a);case.sortie=root/'invite'
            return run(case,env,scenario=scenario,rpc_hook=rpc_hook)
        finally:
            (root/'r2-duplex-resultats.json').write_text(json.dumps(report,indent=2)+'\n')
if __name__=='__main__':raise SystemExit(main())
