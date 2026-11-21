"""Recette de connexion uniquement ; aucun cycle mémoire ni lancement ELF."""
import argparse,json,re,time
from pathlib import Path
from contextlib import ExitStack
from recuperation_qemu import fixture,run,SERVER,DOMAIN
from exec_deux_vm import route,signature
import admission_exec as execs
from reprise import Refus

from auth_records import decisions

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('arch',choices=['aarch64','x86_64']);p.add_argument('sector',type=int,choices=[512,4096])
    for name in ('iso','registre','genese','cles','decision','secrets','sortie','detenteur','capture','triplet'):
        p.add_argument('--'+name,type=lambda v:Path(v).resolve(),required=True)
    p.add_argument('--tentatives',type=int,default=10)
    p.add_argument('--expire-premier',action='store_true')
    p.add_argument('--retard-secondes',type=float,default=1.6)
    a=p.parse_args()
    if not 0<a.retard_secondes<=4:raise Refus('retard de panne hors borne')
    if not 1<=a.tentatives<=12:raise Refus('borne du témoin de connexion : 1..12')
    a.mode='nominal';a.vue=a.activation=a.exec_admission=True;a.vfs=a.mutation=False;a.lecture_privee='autorisee'
    a.legacy_disk=a.genese.with_name('autorite.raw');a.sortie.mkdir(parents=True,exist_ok=False);a.secrets.mkdir(mode=0o700,parents=True,exist_ok=False)
    report={'succes':False,'tentatives':[],'aucun_lancement_demande':True}
    fault={'tentative':None,'consultations':0,'injectee':False}
    def before(disk,out,control,request,phase):
        if phase=='consultation' and fault['tentative']==0:
            fault['consultations']+=1
            if a.expire_premier and fault['consultations']==2:
                # Retard de réponse explicite : le temps invité continue, aucun
                # changement d'horloge/de délai, aucune mutation rejouée.
                fault['injectee']=True;fault['debut_ns']=time.monotonic_ns()
                time.sleep(a.retard_secondes);fault['fin_ns']=time.monotonic_ns()
    def scenario(serial,control,disk,out):
        serial.expect(rb'NAEXEC registre pret puis connexion du vrai nsh',100)
        if b'nsh> ' not in serial.data:serial.expect(rb'nsh> ',20)
        previous=None
        try:
            for n in range(a.tentatives):
                fault['tentative']=n
                start=serial.offset;serial.submit('true' if n==0 else 'session reauthentifier');serial.expect(rb'nsh> ',30)
                raw=bytes(serial.data[start:serial.offset]);(out/f'connexion-{n+1}.bin').write_bytes(raw)
                ds=decisions(raw)
                if len(ds)!=1:raise Refus('décision native de connexion absente ou multiple')
                d=ds[0];report['tentatives'].append(d)
                if d.get('defi'):
                    if d['defi']=='0'*64 or d['defi']==previous:raise Refus('défi absent ou réutilisé')
                    previous=d['defi']
                    if d['echeance']-d['emission']!=1500:raise Refus('échéance défi modifiée')
                if d['retour']:
                    expired=d.get('expire',False) or d.get('verification',0)>=d.get('echeance',2**64-1)
                    if not expired:raise Refus('refus autre que défi expiré')
                else:
                    status,_=serial.command('true')
                    if status:raise Refus('commande après authentification refusée')
            if a.expire_premier and (not fault['injectee'] or report['tentatives'][0]['retour']!=5 or report['tentatives'][1]['retour']!=0):raise Refus('expiration suivie de reprise explicite non démontrée')
            report['injection']=fault.copy()
            if report['tentatives'][-1]['retour']:raise Refus('dernière authentification refusée')
            # Les expirations ne deviennent jamais des succès ; une tentative
            # ultérieure doit explicitement réussir avec une autre identité.
            report['refus_suivis_de_succes']=all(any(y['retour']==0 for y in report['tentatives'][i+1:]) for i,x in enumerate(report['tentatives']) if x['retour'])
            report['succes']=report['refus_suivis_de_succes'] and b'NAEXEC enfant' not in serial.data
            return report
        finally:(out/'authentifications.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    with ExitStack() as stack:
        env=fixture(a,stack);dispatcher,holders,admin=env;api=execs.Admissions(dispatcher.authority.authority);execs.initialize(api.path)
        old=route(api);api.enroll(SERVER,DOMAIN,old,signature(admin,'enroll',old));api.begin_withdrawal(SERVER,DOMAIN,old,signature(admin,'withdraw',old));api.drain(SERVER,DOMAIN,old)
        a.sortie=a.sortie/"invite"
        rc=run(a,env,scenario=scenario,before_mutation=before)
        d=json.loads((a.sortie/'resultat.json').read_text())
        if not (d.get('drainage_signataire') and d.get('drainage_registre') and d.get('plafond_respecte') and d.get('cinquieme_racine_refusee')):raise Refus('drainage ou quotas non démontrés')
        return rc
if __name__=='__main__':raise SystemExit(main())
