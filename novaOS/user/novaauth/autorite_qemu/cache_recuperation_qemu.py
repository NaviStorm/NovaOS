"""Injections de récupération du cache : vrai registre QEMU, aucune autorisation hôte de VFS.

Le changement de tête utilise l'API durable de laboratoire et sa clé de fixture,
jamais une écriture directe dans la base de l'autorité.
"""
from contextlib import ExitStack,closing
from pathlib import Path
import argparse,hashlib,json,time,re
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from recuperation_qemu import fixture,run,SERVER,DOMAIN
from reprise import open_database,Refus

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('arch',choices=['aarch64','x86_64'])
    p.add_argument('sector',type=int,choices=[512,4096])
    p.add_argument('attaque',choices=['tete-modifiee','fragment-altere','fragment-substitue','expiration','reponse-perdue'])
    for name in ('iso','registre','genese','cles','decision','secrets','sortie','detenteur','capture','triplet','ancienne-autorite'):
        p.add_argument('--'+name,type=lambda v:Path(v).resolve(),required=True)
    a=p.parse_args()
    a.mode='nominal';a.vue=a.activation=a.vfs=True;a.mutation=False
    a.lecture_privee='refusee';a.legacy_disk=a.ancienne_autorite
    a.sortie.mkdir(parents=True,exist_ok=False)
    a.secrets.mkdir(parents=True,exist_ok=False,mode=0o700)
    root=a.sortie
    report={'attaque':a.attaque,'injection':False,'succes':False,
        'portee':'récupération et refus VFS invités sous injection ; parcours positif qualifié séparément',
        'banc_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    with ExitStack() as stack:
        environment=fixture(a,stack)
        dispatcher,holders,admin=environment
        chunks=next(iter(dispatcher.ciphertexts.values()))
        total=len(chunks)
        if a.attaque in ('fragment-altere','fragment-substitue'):
            original=chunks[0]
            chunks[0]=bytes([original[0]^1])+original[1:] if a.attaque=='fragment-altere' else chunks[1]
            report['injection']=True
        received=0
        def hook(phase,operation,body,result,control,disk):
            nonlocal received
            if phase=='before' and a.attaque=='expiration':
                # Faire progresser les échanges : une pause unique de 11 s
                # déclenche le délai par fragment de 2 s, avant la borne globale.
                report['injection']=True
                report['retards_600ms']=report.get('retards_600ms',0)+1
                time.sleep(0.6)
            if phase=='before' and operation==19 and not report['injection'] and a.attaque=='reponse-perdue':
                report['injection']=True
                raise Refus('UNCERTAIN : réponse ciphertext perdue volontairement')
            if phase!='after' or operation!=19:return
            received+=1
            if a.attaque=='tete-modifiee' and received==total:
                authority=dispatcher.authority.authority
                with closing(open_database(authority.path)) as db:
                    state=authority._current(db,SERVER,DOMAIN)
                op=b'cache-head-test!'
                head=hashlib.sha256(b'tete publique synthetique differente'+state.head).digest()
                key=Ed25519PrivateKey.from_private_bytes((a.cles/'serveur').read_bytes())
                receipt=authority.publish(state,op,head,key.sign(state.mutation(op,head)))
                (root/'mutation-tete.recu').write_bytes(receipt)
                report.update(injection=True,generation_avant=state.generation,generation_apres=state.generation+1,
                    empreinte_avant=state.head.hex(),empreinte_apres=head.hex())
        def scenario(serial,control,disk,out):
            serial.expect(rb'NARV quarantaine terminee',100)
            raw=serial.data
            auth=(12).to_bytes(8,'little')+bytes(16)
            denial=(5).to_bytes(8,'little')+bytes(16)
            refused=(b'NARV Alice authentification '+auth.hex().encode() in raw and
                b'NARV Alice P quarantaine '+denial.hex().encode() in raw and
                b'NARV VFS refus octets=0' in raw and b'NARV VFS lecture' not in raw)
            report['statut_authentification']=12 if refused else None
            report['statut_vfs']=5 if refused else None
            return {'succes':refused,'refus_recuperation':refused,'octets_prives':0 if refused else None}
        a.sortie=root/'invite'
        run(a,environment,scenario=scenario,rpc_hook=hook)
        result_path=a.sortie/'resultat.json'
        result=json.loads(result_path.read_text())
        raw=(a.sortie/'invite.serial').read_bytes()
        rows=[list(map(int,s.split())) for s in (a.sortie/'rpc-public.log').read_text().splitlines()]
        report.update(statut_natif=result.get('statut_natif'),
            activation_absente=not any(r[1]==18 for r in rows),
            preparation_absente=b'NAR2 preparation P9 relue' not in raw,
            cache_composant=b'NAR2 tete courante authentifiee cache-tests=OK' in raw,
            ressources_registre=result.get('ressources_registre'),ressources_signataire=result.get('ressources'),
            drainage_registre=result.get('drainage_registre'),drainage_signataire=result.get('drainage_signataire'),
            plafond=result.get('plafond_respecte'),resultat_invite=result.get('succes'),
            debut_ms=result.get('debut_ms'),fin_ms=result.get('fin_ms'),echeance_ms=result.get('echeance_ms'),
            demandes_ciphertext=sum(r[1]==19 for r in rows))
        signer=re.search(rb'DEBUG pid=([0-9]+) NAUTH signataire NOW autorise',raw)
        terminated=bool(signer and re.search(rb'PROC pid='+signer[1]+rb' niveau=.* sortie=(?:[1-9][0-9]*)',raw))
        counters=result.get('ressources_registre',{})
        before,after=counters.get('avant'),counters.get('apres')
        released=bool(terminated and before and after and after[2]==0 and all(x<=y for x,y in zip(after,before)))
        report['signataire_termine']=terminated
        report['restitution_apres_terminaison']=released
        report['succes']=bool(report['injection'] and result.get('succes') and report['activation_absente']
            and report['preparation_absente'] and report['cache_composant'] and (report['drainage_registre'] or released)
            and report['drainage_signataire'] and report['plafond'])
        if a.attaque=='expiration':
            report['succes']=report['succes'] and report['fin_ms']>=report['echeance_ms']
        if a.attaque=='reponse-perdue':
            report['succes']=report['succes'] and report['demandes_ciphertext']==1
        result['portee']=report['portee']
        result_path.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    (root/'bilan-cache.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    return 0 if report['succes'] else 1

if __name__=='__main__':raise SystemExit(main())
