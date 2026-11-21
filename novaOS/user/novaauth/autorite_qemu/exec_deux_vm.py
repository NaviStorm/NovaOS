"""Banc de tickets invités ; une autorité durable hors des disques des VM.

Le mode de diagnostic initial ne revendique pas la qualification à deux VM.
"""
from contextlib import ExitStack,closing
from pathlib import Path
import argparse,copy,json,os,re,shutil,time,threading
from exec_vm_debug import Debug,qmp,symbol
from exec_resources import verify as verify_resources
from horloge_pause_qemu import Clock
from rpc_recuperation import Dispatcher
from recuperation import digest
from recuperation_qemu import fixture,run,SERVER,DOMAIN
from reprise import open_database,Refus
import admission_exec as execs

# Attendre la ligne complète : la réception série peut couper le dernier nombre.
REFUS_TICKET = rb'NAEXEC refus owner=([0-9]+) ticket=([0-9]+) statut=([0-9]+) maintenant=([0-9]+) echeance=([0-9]+)\r?\n'

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
    mode=p.add_mutually_exclusive_group(required=True)
    mode.add_argument('--diagnostic',action='store_true')
    mode.add_argument('--deux-vm',action='store_true')
    p.add_argument('--kernel',type=lambda v:Path(v).resolve())
    p.add_argument('--admis-retarde',action='store_true')
    p.add_argument('--course-retrait',choices=['admission-premiere','retrait-premier'])
    p.add_argument('--indisponible-avant-admission',action='store_true')
    p.add_argument('--refus-attendu',choices=['installation','autorite-indisponible'])
    p.add_argument('--perte-reponse',choices=['admission','issue'])
    p.add_argument('--sonde-ticket',choices=['recu','expire','revoque','concurrent'])
    a=p.parse_args();a.mode='nominal';a.vue=a.activation=a.exec_admission=True
    a.vfs=a.mutation=False;a.lecture_privee='autorisee'
    adapter=Path(os.environ['NOVA_ADAPTATEUR']).resolve(strict=True)
    if not adapter.is_file() or not os.access(adapter,os.X_OK):
        raise Refus('adaptateur natif absent ou non exécutable')
    a.sortie.mkdir(parents=True,exist_ok=False);a.secrets.mkdir(parents=True,exist_ok=False,mode=0o700)
    root=a.sortie
    if a.deux_vm and (a.refus_attendu or a.sonde_ticket not in (None,'concurrent')):
        raise Refus('sonde incompatible avec le parcours deux VM')
    if a.deux_vm and (a.perte_reponse=='issue' or a.sonde_ticket=='concurrent' or a.course_retrait=='admission-premiere'):
        a.admis_retarde=True
    if a.indisponible_avant_admission and (a.admis_retarde or a.course_retrait):raise Refus('pannes distinctes requises')
    if a.deux_vm and a.kernel is None:raise Refus('ELF noyau exact requis pour les points QEMU')
    a.legacy_disk=a.genese.with_name('autorite.raw')
    if a.deux_vm:a.pause_transport=Clock()
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
            marker=serial.expect(rb'NAEXEC registre pret puis connexion du vrai nsh|NARV registre quarantaine generation non activable',100)
            if b'quarantaine' in marker[0]:raise Refus('registre en quarantaine ; aucun ticket créé')
            if b'nsh> ' not in serial.data:
                serial.expect(rb'nsh> ',20)
            status,output=serial.command('true')
            if status:raise Refus(f'authentification shell : {status}')
            first=route(api)
            if a.refus_attendu=='autorite-indisponible':a.autorite_hors_ligne=True;case.autorite_hors_ligne=True
            command=('ticket-test '+a.sonde_ticket+' cut --version') if a.sonde_ticket else 'cut --version'
            status,output=serial.command(command)
            (out/'commande-cut.bin').write_bytes(output)
            with closing(open_database(api.path)) as db:
                rows=db.execute('SELECT hex(op),state,hex(child),sequence,terminal_sequence FROM exec_admissions ORDER BY sequence').fetchall()
            expected=0 if a.sonde_ticket in ('expire','revoque') else 1
            success=status==0 and len(rows)==expected and all(r[1]==1 for r in rows)
            if a.refus_attendu:
                success=status!=0 and not rows and b'NAEXEC enfant ' not in serial.data
            if a.perte_reponse:
                log=(out/'rpc-public.log').read_text()
                operations=[int(line.split()[1]) for line in log.splitlines()]
                success=success and injection['effectuee'] and operations.count(40)==1 and operations.count(41)==1 and 42 in operations
            return {'succes':success,'commande':command,'statut_cut':status,'incarnation':first.hex(),'admissions':rows,'deux_VM':False,'perte_reponse':dict(injection),'refus_attendu':a.refus_attendu}
        injection={'effectuee':False,'etape':a.perte_reponse}
        race={'mode':a.course_retrait,'commencee':False,'terminee':False}
        admitted=threading.Event();race_thread=None
        def withdrawal(instance):
            try:
                race['invocation_ns']=time.monotonic_ns()
                if a.course_retrait=='admission-premiere' and not admitted.wait(30):raise Refus('admission non observée')
                data,sig=api.begin_withdrawal(SERVER,DOMAIN,instance,signature(admin,'withdraw',instance))
                (root/'retrait-en-cours.recu').write_bytes(data+sig)
                race['terminee']=True;race['acquittement_ns']=time.monotonic_ns()
            except Exception as error:race['erreur']=repr(error)
        def join_race():
            race_thread.join(30)
            if race_thread.is_alive() or not race['terminee']:raise Refus('retrait concurrent non établi : '+str(race))
        def hook(phase,operation,body,result,control,disk):
            nonlocal race_thread
            if a.course_retrait and operation==40:
                if phase=='before' and not race['commencee']:
                    race['commencee']=True;race['requete_signee_ns']=time.monotonic_ns()
                    race_thread=threading.Thread(target=withdrawal,args=(body[32:64],))
                    race_thread.start()
                    if a.course_retrait=='retrait-premier':join_race()
                if phase=='after' and a.course_retrait=='admission-premiere':
                    race['admission_durable_ns']=time.monotonic_ns();admitted.set();join_race()
            target=40 if a.perte_reponse=='admission' else 41
            if a.perte_reponse and phase=='after' and operation==target and not injection['effectuee']:
                injection['effectuee']=True
                raise Refus('UNCERTAIN : reçu durable volontairement perdu par le banc')
        if a.diagnostic:
            case=copy.copy(a);case.sortie=root/'diagnostic-A'
            return run(case,environment,scenario=scenario,rpc_hook=hook)
        report={'succes':False,'deux_VM':True,'architecture':a.arch,'secteur':a.sector,
            'limite':'QEMU ; une autorité indépendante, conservée hors restauration',
            'course':race,'perte_reponse':injection,'consommation_concurrente':a.sonde_ticket=='concurrent',
            'autorite_indisponible':a.indisponible_avant_admission}
        def rows():
            with closing(open_database(api.path)) as db:
                return db.execute('SELECT hex(op),state,hex(child),sequence,terminal_sequence FROM exec_admissions ORDER BY sequence').fetchall()
        def routes():
            with closing(open_database(api.path)) as db:
                return [(i.hex(),s) for i,s in db.execute('SELECT incarnation,state FROM exec_routes ORDER BY incarnation')]
        def ready(serial,reference=None,out=None):
            marker=serial.expect(rb'NAEXEC registre pret puis connexion du vrai nsh|NARV registre quarantaine generation non activable',100)
            if b'quarantaine' in marker[0]:raise Refus('registre en quarantaine ; aucun ticket créé')
            if b'nsh> ' not in serial.data:serial.expect(rb'nsh> ',20)
            if reference is not None:
                if not any(i==report['incarnation_A'] and state=='RETIREE' for i,state in routes()):
                    raise Refus('rejeu B avant retrait durable de A')
                report['rejeu_B']=replay(serial,reference,out)
            start=len(serial.data)
            status,_=serial.command('true')
            if status and b'NXLOGIN refus defi expire' in serial.data[start:]:
                # Nouvelle commande explicite, un seul nouvel essai ; aucune
                # répétition d'une mutation ni réutilisation du défi expiré.
                report.setdefault('defis_expires',[]).append({'instance':str(out),'statut':status})
                status,_=serial.command('true')
            if status:raise Refus('authentification initiale refusée')
        def replay(serial,key,out):
            if b'nsh> ' not in serial.data:serial.expect(rb'nsh> ',100)
            if b'NXBAIL op=NSHOPEN1' in serial.data or b'phase=avant_ticket' in serial.data:
                raise Refus('authentification ou ticket déjà émis avant la sonde')
            start=len(serial.data)
            serial.submit(f'ticket-reference {key}')
            result=serial.expect(rb'NXREFERENCE ticket=([0-9]+) statut=([0-9]+) valeur=([0-9]+) appels=1',30)
            serial.expect(rb'nsh> ',20)
            native=list(map(int,result.groups()))
            trace=bytes(serial.data[start:])
            (out/'rejeu-reference.bin').write_bytes(trace)
            if native[0]!=key or native[1]!=5 or native[2]!=0:
                raise Refus(f'rejeu avant authentification : résultat inattendu {native}')
            # DENIED vient de la recherche native dans la table vide du nouveau
            # processus, pas de l'expiration ni d'un garde d'authentification.
            budgets=re.findall(rb'NAEXEC ressources phase=(consommation|refus_sans_creation) (.*)',trace)
            if len(budgets)!=2 or budgets[0][0]!=b'consommation' or budgets[1][0]!=b'refus_sans_creation' or budgets[0][1]!=budgets[1][1]:
                raise Refus('ressources du rejeu non restituées ou mesure absente')
            if b'NAEXEC enfant' in trace or b'NAEXEC attente' in trace:
                raise Refus('ticket ou enfant inattendu pendant le rejeu')
            serial.submit(f'ticket-reference {key}')
            serial.expect(rb'NXREFERENCE sonde fermee aucun appel SPAWN',20)
            serial.expect(rb'nsh> ',20)
            if serial.data[start:].count(b'phase=consommation')!=1:
                raise Refus('sonde consommée plus d’une fois')
            return {'reference':key,'statut_natif':native[1],'valeur':native[2],
                'appels_SPAWN':1,'ressources':verify_resources(trace,probe=True),'ressources_identiques':True,'enfants':0,'seconde_sonde_refusee':True}
        def finish_a(serial,negative=False,terminal=False):
            serial.expect(rb'nsh> ',35)
            serial.submit('echo NXA_STATUT_$?')
            if a.indisponible_avant_admission:
                serial.expect('saisie refusée : accès refusé'.encode(),20)
                serial.expect(rb'nsh> ',20)
                report['commande_suivante_refusee_sans_autorite']=True
                report['statut_shell_non_consultable']='nsh refuse la commande de consultation ; voir statut natif'
                return
            value=int(serial.expect(rb'NXA_STATUT_([0-9]+)\r?\n',20)[1]);serial.expect(rb'nsh> ',20)
            report['statut_commande_A']=value
            if not terminal and (value==0)==negative:raise Refus('statut de A inattendu')
            if a.sonde_ticket=='concurrent':
                workers=re.findall(rb'NXEXEC concurrence ticket=([0-9]+) thread=([0-9]+) statut=([0-9]+) droit_collecte=([0-9]+) anciens_spawns_refuses=([0-9]+)/3',serial.data)
                report['threads_A']=[list(map(int,r)) for r in workers]
                if len(workers)!=2 or sorted(int(r[2]) for r in workers)!=[0,12] or len({r[1] for r in workers})!=2 or any(int(r[0])!=report['reference_A'] or r[4]!=b'3' for r in workers):
                    raise Refus('double consommation non qualifiée')
                if len(re.findall(rb'NAEXEC enfant ',serial.data))!=1:raise Refus('nombre enfants concurrence incorrect')
        def first(serial,control,disk,out):
            ready(serial);old=route(api);report['incarnation_A']=old.hex()
            qmp(control/'qmp.sock','stop')
            debug=Debug(control/'gdb.sock',serial,a.arch)
            try:
                point=symbol(a.kernel,'novaos::sched::process::ipc::exec::authority::start' if a.admis_retarde else 'novaos::sched::process::ipc::exec::consume')
                debug.breakpoint(point);debug.resume()
                serial.submit('ticket-test concurrent cut --version' if a.sonde_ticket=='concurrent' else 'cut --version')
                report['arret_A']=debug.stopped();a.pause_transport.pause();debug.breakpoint(point,False)
                if a.admis_retarde:
                    keys=re.findall(rb'NAEXEC attente owner=[0-9]+ ticket=([0-9]+) maintenant=',serial.data)
                    if not keys:raise Refus('référence historique absente')
                    report['reference_A']=int(keys[-1])
                else:
                    report['reference_A']=debug.register(0 if a.arch=='aarch64' else 5)
                report['qmp_A_suspendue']=qmp(control/'qmp.sock','query-status')
                if report['qmp_A_suspendue']['running']:raise Refus('A non suspendue')
                if a.admis_retarde:
                    if len(rows())!=1 or rows()[0][1]!=0:raise Refus('admission en attente non démontrée')
                elif rows():raise Refus('ticket A déjà admis')
                if b'NAEXEC enfant' in serial.data:raise Refus('enfant A prématuré')
                with disk.open('rb') as f:os.fsync(f.fileno())
                shutil.copyfile(disk,root/'instantane.p9')
                shutil.copyfile(out/'ancienne-autorite-locale.raw',root/'instantane-ancienne-autorite.raw')
                if a.perte_reponse=='admission':
                    # Une réponse perdue peut conduire à Created OU à Cancelled.
                    # Attendre l'acquittement terminal natif et vérifier le reçu
                    # durable, jamais déduire une annulation d'un timeout.
                    a.pause_transport.resume();debug.resume()
                    serial.expect(rb'NAEXEC transition registry=([0-9]+) owner=([0-9]+) kind=3 value=([12])\r?\n',35)
                    finish_a(serial,terminal=True)
                    terminal=rows()
                    if len(terminal)!=1:raise Refus('issue unique absente après réponse perdue')
                    _,state,child,position,settled=terminal[0]
                    children=re.findall(rb'NAEXEC enfant owner=([0-9]+) child=([0-9]+)',serial.data)
                    if state not in (1,2) or settled is None or settled<=position:
                        raise Refus('issue non durable ou mal ordonnée')
                    if state==2 and (child!='00'*32 or children or report['statut_commande_A']==0):
                        raise Refus('annulation non démontrée')
                    if state==1 and (child=='00'*32 or len(children)!=1 or report['statut_commande_A']!=0):
                        raise Refus('création/collecte non démontrée')
                    report['issue_A_avant_retrait']=terminal
                    report['A_achevee_avant_RETIREE']=terminal
                    qmp(control/'qmp.sock','stop');a.pause_transport.pause()
                if a.course_retrait=='retrait-premier' or a.indisponible_avant_admission:
                    if a.indisponible_avant_admission:case.autorite_hors_ligne=True
                    a.pause_transport.resume();debug.resume()
                    rejected=serial.expect(REFUS_TICKET,30)
                    report['refus_A_avant_drainage']=list(map(int,rejected.groups()))
                    _,key,code,now,until=report['refus_A_avant_drainage']
                    if key!=report['reference_A'] or code!=10 or now>=until:
                        raise Refus('refus avant expiration non établi')
                    finish_a(serial,negative=True)
                    if b'NAEXEC enfant ' in serial.data:raise Refus('enfant interdit après refus')
                    if a.indisponible_avant_admission:
                        if rows():raise Refus('admission sans autorité')
                        # Seul le transport A était coupé, sans rétablir son signataire.
                        report['absence_admission_hors_ligne']=True
                    else:
                        if len(rows())!=1 or rows()[0][1]!=2:raise Refus('annulation irréversible non attestée')
                        report['annulation_A']=rows()
                    report['A_refusee_avant_RETIREE']=True
                    qmp(control/'qmp.sock','stop');a.pause_transport.pause()
                if not race['terminee']:
                    data,sig=api.begin_withdrawal(SERVER,DOMAIN,old,signature(admin,'withdraw',old))
                    (root/'retrait-en-cours.recu').write_bytes(data+sig)
                report['routes_pendant_retrait']=routes()
                manifest=(root/'manifeste.bin').read_bytes()
                keys=[holders[0].keys[0],holders[1].keys[0],holders[0].keys[1],holders[1].keys[1],holders[0].keys[2],holders[1].keys[2]]
                admin.call('mandate',SERVER,DOMAIN,dispatcher.peer_key,dispatcher.measurement,digest(manifest),
                    int.from_bytes(manifest[78:86],'big'),manifest[270:302],keys)
                cached={}
                def approve(c,server,pop):
                    if c.op in cached:return cached[c.op]
                    signature=dispatcher.approve_independently(c,server,pop)
                    try:dispatcher.authority.approve(c,signature,server,pop)
                    except Refus as error:
                        if 'drainage EXEC' not in str(error):raise
                        report['B_bloquee_pendant_retrait']=str(error)
                    else:raise Refus('B admise avant RETIREE')
                    if route(api)!=old:raise Refus('route modifiée malgré refus B')
                    report['admissions_avant_drainage']=rows()
                    if a.admis_retarde:
                        try:api.drain(SERVER,DOMAIN,old)
                        except Refus as error:
                            if 'drainage incomplet' not in str(error):raise
                            report['drainage_bloque']=str(error)
                        else:raise Refus('drainage prématuré avec création encore possible')
                        raise Refus('drainage EXEC incomplet : reprise B à demander explicitement')
                    data,sig=api.drain(SERVER,DOMAIN,old)
                    (root/'retrait-definitif.recu').write_bytes(data+sig)
                    report['routes_apres_drainage']=routes()
                    cached[c.op]=signature;return signature
                second_dispatcher=Dispatcher(dispatcher.authority,dispatcher.enrollment,SERVER,DOMAIN,
                    dispatcher.peer_key,dispatcher.measurement,dispatcher.manifests,dispatcher.ciphertexts,
                    approve,dispatcher.deliver_independently,dispatcher.retrieve_independently)
                def second(t,c,d,o):
                    ready(t,report['reference_A'],o);new=route(api)
                    if new==old:raise Refus('incarnation B identique à A')
                    report['incarnation_B']=new.hex()
                    status,output=t.command('cut --version');(o/'cut-version.bin').write_bytes(output)
                    report['statut_B']=status;report['admissions_B']=rows()
                    expected=2 if a.admis_retarde or a.course_retrait=='retrait-premier' or a.perte_reponse=='admission' else 1
                    states=sorted(r[1] for r in rows())
                    wanted=sorted([1,report['issue_A_avant_retrait'][0][1]]) if a.perte_reponse=='admission' else ([1,2] if a.course_retrait=='retrait-premier' else [1]*expected)
                    if status!=0 or len(rows())!=expected or states!=wanted:raise Refus('lancement B non confirmé')
                    # Deuxième lancement : vérifier le plateau du pool IPC
                    # après son premier agrandissement légitime.
                    warm_status,warm_output=t.command('cut --version')
                    (o/'cut-version-chaud.bin').write_bytes(warm_output)
                    if warm_status!=0 or len(rows())!=expected+1 or rows()[-1][1]!=1:
                        raise Refus('second lancement B non confirmé')
                    report['statut_B_chaud']=warm_status
                    # Exclure les deux sondes historiques du bilan des lancements B.
                    begin=bytes(t.data).index(b'NAEXEC ressources phase=avant_ticket')
                    report['ressources_B']=verify_resources(bytes(t.data[begin:]),warm=True)
                    # B demeure vivante pendant le réveil de A.
                    if report.get('A_refusee_avant_RETIREE'):
                        report['succes']=True
                        return {'succes':True,'statut_cut':status,'A_refusee_avant_RETIREE':True}
                    if a.admis_retarde or a.perte_reponse=='admission':
                        report['succes']=True
                        return {'succes':True,'incarnation':new.hex(),'statut_cut':status,'deux_VM':True,'A_achevee_avant_RETIREE':report['A_achevee_avant_RETIREE']}
                    a.pause_transport.resume();debug.resume()
                    serial.expect(REFUS_TICKET,30)
                    serial.expect(rb'nsh> ',20)
                    refused=re.search(REFUS_TICKET,serial.data)
                    report['refus_A']=list(map(int,refused.groups()))
                    report['enfants_A']=len(re.findall(rb'NAEXEC enfant ',serial.data))
                    if report['refus_A'][1]!=report['reference_A'] or report['refus_A'][2]!=10 or report['refus_A'][3]>=report['refus_A'][4] or report['enfants_A']!=0:raise Refus('référence, statut de retrait, expiration ou création A interdite')
                    if b'NAUTH incarnation locale retiree' not in serial.data:raise Refus('cause retrait non démontrée')
                    report['admissions_apres_reveil_A']=rows()
                    if len(rows())!=expected+1:raise Refus('admission supplémentaire après retrait')
                    report['succes']=True
                    return {'succes':True,'incarnation':new.hex(),'statut_cut':status,'deux_VM':True}
                b=copy.copy(a);b.pause_transport=Clock();b.sortie=root/'VM-B';b.genese=root/'instantane.p9';b.legacy_disk=root/'instantane-ancienne-autorite.raw'
                if a.admis_retarde:
                    b.sortie=root/'VM-B-retrait-en-cours'
                    blocked=run(b,(second_dispatcher,holders,admin),scenario=second)
                    report['retour_B_pendant_drainage']=blocked
                    if blocked==0 or not report.get('drainage_bloque') or route(api)!=old:
                        raise Refus('blocage réel de B non démontré')
                    # L'échange B refusé est terminé ; ne pas immobiliser un TLS
                    # de récupération pendant la réconciliation de A.
                    a.pause_transport.resume();debug.resume()
                    serial.expect(rb'NAEXEC enfant owner=([0-9]+) child=([0-9]+)',20)
                    serial.expect(rb'NAEXEC transition registry=([0-9]+) owner=([0-9]+) kind=3 value=1',20)
                    if len(rows())!=1 or rows()[0][1]!=1:raise Refus('enfant A non enregistré durablement')
                    report['A_achevee_avant_RETIREE']=rows()
                    finish_a(serial)
                    qmp(control/'qmp.sock','stop');a.pause_transport.pause()
                    data,sig=api.drain(SERVER,DOMAIN,old)
                    (root/'retrait-definitif.recu').write_bytes(data+sig)
                    report['routes_apres_drainage']=routes()
                    admin.call('mandate',SERVER,DOMAIN,dispatcher.peer_key,dispatcher.measurement,digest(manifest),
                        int.from_bytes(manifest[78:86],'big'),manifest[270:302],keys)
                    report['nouvelle_approbation_B_explicite']=True
                    second_dispatcher=Dispatcher(dispatcher.authority,dispatcher.enrollment,SERVER,DOMAIN,
                        dispatcher.peer_key,dispatcher.measurement,dispatcher.manifests,dispatcher.ciphertexts,
                        dispatcher.approve_independently,dispatcher.deliver_independently,dispatcher.retrieve_independently)
                    b.sortie=root/'VM-B'
                report['retour_VM_B']=run(b,(second_dispatcher,holders,admin),scenario=second)
                report['succes']=report['succes'] and report['retour_VM_B']==0
                report['ressources_A']=verify_resources(bytes(serial.data))
                if a.perte_reponse:
                    operations=[int(line.split()[1]) for line in (out/'rpc-public.log').read_text().splitlines()]
                    if not injection['effectuee'] or operations.count(40)!=1 or operations.count(41)!=1 or 42 not in operations:
                        raise Refus('réconciliation sans répétition non démontrée')
                    report['operations_RPC_A']=operations
                if a.course_retrait and not race['terminee']:raise Refus('course non exercée')
                with closing(open_database(api.path)) as db:
                    report['ordre_durable']=[(seq,kind) for seq,kind in db.execute('SELECT sequence,kind FROM exec_events ORDER BY sequence')]
                return dict(report)
            except Exception:
                report['succes']=False
                raise
            finally:
                a.pause_transport.resume();debug.close();(root/'bilan-deux-vm.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
        case=copy.copy(a);case.sortie=root/'VM-A'
        result=run(case,environment,scenario=first,rpc_hook=hook)
        if result or not report['succes']:return result or 1
        # L'ancienne VM est maintenant arrêtée. Nouvelle exécution sur les mêmes
        # disques anciens ; aucune approbation de reprise, autorité inchangée.
        def no_approval(*unused):raise Refus('reprise administrative non autorisée pour le rejeu après redémarrage')
        denied=Dispatcher(dispatcher.authority,dispatcher.enrollment,SERVER,DOMAIN,
            dispatcher.peer_key,dispatcher.measurement,dispatcher.manifests,dispatcher.ciphertexts,
            no_approval,dispatcher.deliver_independently,dispatcher.retrieve_independently)
        reboot=copy.copy(a);reboot.pause_transport=Clock();reboot.sortie=root/'VM-A-redemarree'
        reboot.genese=root/'instantane.p9';reboot.legacy_disk=root/'instantane-ancienne-autorite.raw'
        active=route(api);prior=rows()
        def restarted(t,c,d,o):
            report['rejeu_A_redemarree']=replay(t,report['reference_A'],o)
            t.expect(rb'NARV registre quarantaine generation non activable|NAR2 registre termine statut=([0-9a-f]{16})',100)
            closed=rb'NAUTH mesure NAUTH TLS apres roots=\d+ handles=\d+ waits=\d+ pages=\d+ pile=\d+ cadres_ipc_globaux=\d+\r?\n'
            if not re.search(closed,t.data):t.expect(closed,15)
            if route(api)!=active or rows()!=prior or b'NAEXEC enfant' in t.data:
                raise Refus('réactivation ou admission après redémarrage interdit')
            return {'succes':True,'rejeu':report['rejeu_A_redemarree'],'aucune_admission':True,
                'ancienne_generation_non_activee':True}
        result=run(reboot,(denied,holders,admin),scenario=restarted)
        report['retour_A_redemarree']=result;report['succes']=report['succes'] and result==0
        (root/'bilan-deux-vm.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
        return result

if __name__=='__main__':raise SystemExit(main())
