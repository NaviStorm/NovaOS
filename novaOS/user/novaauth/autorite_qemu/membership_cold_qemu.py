"""Q4 : arrêt réel après RESERVE ; refus sûr, sans simuler un ACK d'effecteur.

Ce banc n'écrit jamais les tables d'autorité. Il ne qualifie pas la restauration
complète de disponibilité quand une ancienne instance a disparu.
"""
import copy
import json
import re
from reprise import Refus, transaction
from exec_deux_vm import route, signature
from recuperation_qemu import run, SERVER, DOMAIN


def reservation_snapshot(path, operation):
    with transaction(path) as db:
        row=db.execute('SELECT state,generation,previous,next_head FROM membership_changes WHERE op=?',(operation,)).fetchone()
        effects=db.execute('SELECT kind,instance,acknowledged FROM membership_effectors WHERE op=? ORDER BY kind',(operation,)).fetchall()
        mutations=db.execute('SELECT count(*) FROM mutations WHERE op=?',(operation,)).fetchone()[0]
        pending=db.execute('SELECT count(*) FROM membership_operations WHERE state=0').fetchone()[0]
    if row is None or row[0]!=0 or mutations!=0 or pending!=0:
        raise Refus('Q4 : réservation seule, sans publication et sans admission inconnue, non démontrée')
    if [k for k,_,_ in effects]!=[1,2,4] or any(v!=0 for _,_,v in effects):
        raise Refus('Q4 : inventaire ou absence des trois acquittements non démontrés')
    return {'operation':operation.hex(),'etat':'RESERVEE','generation_attendue':row[1],
            'tete_avant':row[2].hex(),'tete_prevue':row[3].hex(),'mutations':mutations,
            'admissions_non_terminees':pending,
            'effecteurs':[{'classe':k,'instance':i.hex(),'acquitte':bool(v)} for k,i,v in effects]}


def verify_cold_refusal(serial, rpc, refus, native):
    if b'NARV registre quarantaine generation non activable' not in serial:
        raise Refus('Q4 : refus du registre invité non observé')
    rows=[list(map(int,line.split())) for line in rpc.splitlines()]
    attempts=[r for r in rows if r[1]==14]
    if len(attempts)!=1 or attempts[0][4]!=5 or native!=5:
        raise Refus('Q4 : refus natif 5 de la nouvelle incarnation non établi')
    if 'drainage EXEC non confirmé : nouvelle incarnation inactive' not in refus:
        raise Refus('Q4 : premier point de refus différent du drainage')
    for marker in (b'NAR2 vue registre isole',b'NAR2 triplet dechiffre',
                   b'NAEXEC registre pret puis connexion du vrai nsh',b'NARM retrait selectif RETIREE'):
        if marker in serial:raise Refus('Q4 : activation ou succès inattendu après reprise froide')


def cold_reserved(case, environment, root, operation, stack):
    dispatcher,holders,admin=environment
    api=__import__('admission_exec').Admissions(dispatcher.authority)
    before=reservation_snapshot(api.path,operation)
    old=route(api)
    packet,sig=api.begin_withdrawal(SERVER,DOMAIN,old,signature(admin,'withdraw',old))
    (root/'retrait-froid-en-cours.bin').write_bytes(packet+sig)
    try:api.drain(SERVER,DOMAIN,old)
    except Refus as e:
        drain_error=str(e)
        if drain_error!='UNCERTAIN : drainage sélectif incomplet, nouvelle incarnation inactive':raise
    else:raise Refus('Q4 : retrait définitif inventé malgré réservation incomplète')
    with transaction(api.path) as db:
        head_before=db.execute('SELECT instance,epoch,generation,head FROM scope WHERE server=? AND domain=?',(SERVER,DOMAIN)).fetchone()
        audits_before=db.execute('SELECT count(*) FROM audit').fetchone()[0]
    # Le triplet n'a pas changé : la sauvegarde initiale reste exacte. Une
    # nouvelle conversation cryptographique remplace seulement l'état volatil
    # du relais, jamais celui de l'autorité indépendante.
    from rpc_recuperation import Dispatcher
    from recuperation import digest
    manifest=(root/'manifeste.bin').read_bytes()
    keys=[holders[0].keys[0],holders[1].keys[0],holders[0].keys[1],holders[1].keys[1],holders[0].keys[2],holders[1].keys[2]]
    # Nouvelle approbation administrative explicite de laboratoire. Elle ne
    # franchit pas la barrière de drainage de l'autorité.
    admin.call('mandate',SERVER,DOMAIN,dispatcher.peer_key,dispatcher.measurement,digest(manifest),
        int.from_bytes(manifest[78:86],'big'),manifest[270:302],keys)
    resumed=Dispatcher(dispatcher.authority,dispatcher.enrollment,SERVER,DOMAIN,
        dispatcher.peer_key,dispatcher.measurement,dispatcher.manifests,dispatcher.ciphertexts,
        dispatcher.approve_independently,dispatcher.deliver_independently,dispatcher.retrieve_independently)
    after=copy.copy(case);after.sortie=root/'redemarrage-reserve';after.genese=case.sortie/'identites.p9'
    facts={}
    def scenario(serial,control,disk,out):
        from administration_collector_qemu import Observer
        observer=Observer(control,after.arch,after.kernel,out);stack.callback(observer.close)
        serial.expect(rb'NARV registre quarantaine generation non activable',65)
        serial.expect(rb'NAEXEC famille_drainee dernier_pid=\d+',20)
        rpc=(out/'rpc-public.log').read_text()
        attempts=[list(map(int,line.split())) for line in rpc.splitlines() if int(line.split()[1])==14]
        native=attempts[0][4] if len(attempts)==1 else None
        verify_cold_refusal(serial.data,rpc,(out/'refus.log').read_text(),native)
        released={}
        for role,service in [('registre',22),('signataire',23)]:
            pids=set(re.findall(rb'NAUTH phase=23 pid=(\d+) service='+str(service).encode()+rb' ',serial.data))
            if len(pids)!=1:raise Refus('Q4 : instance de service ambiguë')
            pid=next(iter(pids))
            if not re.search(rb'NAEXEC restitue pid='+pid+rb' roots=0 handles=0 waits=0 pages=0 pile=0 threads=0',serial.data):
                raise Refus('Q4 : ressources du service non restituées')
            if not re.search(rb'NAEXEC retrait_famille manager=\d+ pid='+pid+rb' service='+str(service).encode()+rb' statut=125',serial.data):
                raise Refus('Q4 : retrait supervisé non établi')
            released[role]={'pid':int(pid),'statut_retrait':125,'roots':0,'handles':0,'waits':0,'pages':0,'pile':0,'threads':0}
        if observer.error:raise Refus('Q4 : collecteur en erreur '+str(observer.error))
        observer.close()
        facts.update(statut_RPC_approbation=native,registre_en_quarantaine=True,ressources_restituees=released,refus_activation=True,aucune_authentification=True,
                     aucun_triplet_active=True,succes=True)
        return dict(facts)
    result=run(after,(resumed,holders,admin),scenario=scenario)
    measures=json.loads((after.sortie/'resultat.json').read_text())
    with transaction(api.path) as db:
        head_after=db.execute('SELECT instance,epoch,generation,head FROM scope WHERE server=? AND domain=?',(SERVER,DOMAIN)).fetchone()
        audits_after=db.execute('SELECT count(*) FROM audit').fetchone()[0]
        states=db.execute('SELECT incarnation,state FROM exec_routes WHERE server=? AND domain=?',(SERVER,DOMAIN)).fetchall()
    preserved=reservation_snapshot(api.path,operation)==before and head_after==head_before and audits_after==audits_before
    if not preserved or route(api)!=old or resumed.instance in (None,old):
        raise Refus('Q4 : réservation, tête ou identité de reprise non conservée')
    if not any(i==old and s=='RETRAIT_EN_COURS' for i,s in states):
        raise Refus('Q4 : ancienne route sans état de retrait en cours')
    complete=bool(result==0 and facts.get('succes') and facts.get('ressources_restituees') and
        measures['drainage_registre'] and measures['plafond_respecte'] and measures['relais_draine'])
    return {**facts,'succes':complete,'reservation':before,'refus_drainage':drain_error,
            'ancienne_incarnation':old.hex(),'nouvelle_incarnation_refusee':resumed.instance.hex(),
            'reservation_inchangee':preserved,'publications':0,'nouveaux_acquittements':0,
            'ancienne_route':'RETRAIT_EN_COURS','activation_nouvelle':False,
            'disponibilite_restauree':False,'resultat_seconde_VM':result,
            'retour_signataire_normal':measures['drainage_signataire'],'retrait_signataire_supervise':bool(facts.get('ressources_restituees')),'drainage_registre':measures['drainage_registre'],
            'plafond_respecte':measures['plafond_respecte']}
