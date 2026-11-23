"""ELF administratifs réels depuis nsh ; parcours indépendants et bornés.

Le lanceur externe détient le verrou commun QEMU. Bob conserve son parcours de
refus historique. Alice commence ensuite une nouvelle authentification initiale,
sans renouvellement implicite. Le lot groupe exerce préparation, confirmation
explicite et consultation du même reçu, sans répétition de mutation.
"""
import argparse
import copy
import hashlib
import json
import re
from contextlib import ExitStack
from pathlib import Path

from recuperation_qemu import fixture, run, SERVER, DOMAIN
from exec_deux_vm import route, signature
from reprise import Refus, transaction
import admission_exec as execs
from administration_epochs import Epochs, initialize
from administration_events import Events, command_status_pattern, observed_command_status_pattern, membership_revision_pattern


def initial_fragments(root, manifest):
    # Les fichiers du provisionnement ont des indices décimaux non rembourrés.
    # Ne jamais placer fragment-10 avant fragment-2.
    if len(manifest)!=418:raise Refus('manifeste initial tronque')
    count=int.from_bytes(manifest[378:382], 'big')
    if not 0<count<=64:raise Refus('nombre de fragments initial hors borne')
    names={f'fragment-{i}.chiffre' for i in range(count)}
    if {p.name for p in root.glob('fragment-*.chiffre')}!=names:
        raise Refus('fragments initiaux incomplets ou surnumeraires')
    chunks=[(root/f'fragment-{i}.chiffre').read_bytes() for i in range(count)]
    if any(not 17<=len(c)<=1040 for c in chunks):raise Refus('fragment initial hors borne')
    return chunks


def command_mode(command, mode):
    if mode == 'interactif':
        return command
    if mode == 'nsh-c':
        import shlex
        return 'nsh -c ' + shlex.quote(command)
    if mode == 'script':
        import shlex
        args=shlex.split(command)
        direct={'whoami':'whoami','id':'id','groups':'groups','id carol':'carol',
                'usermod --disable alice --prepare':'refus'}
        if command in direct:return 'nsh /travail/r2-' + direct[command] + '.nsh'
        if len(args)==3 and args[:2]==['id','--subject-id']:
            return 'export R2_SUBJECT='+shlex.quote(args[2])+'; nsh /travail/r2-renomme.nsh'
        if len(args)==3 and args[0] in ('useradd','usermod','groupadd') and args[1]=='--outcome':
            return 'export R2_REQUEST='+shlex.quote(args[2])+'; nsh /travail/r2-issue-'+args[0]+'.nsh'
        expected={('useradd','--factor-reference','d3e1962bb9c6e8962e338b1972b5359f','--group','base','carol'):'creation',
                  ('groupadd','equipe-r2'):'groupe',('usermod','--rename','robert','bob'):'renommage'}
        suffix=next((i for i,v in enumerate(args) if v in ('--prepare','--confirm')),len(args))
        kind=expected.get(tuple(args[:suffix]))
        if kind and args[-1]=='--prepare':return 'nsh /travail/r2-'+kind+'-preparer.nsh'
        if kind and '--confirm' in args and '--request' in args:
            return 'export R2_CONFIRM='+shlex.quote(args[args.index('--confirm')+1])+' R2_REQUEST='+shlex.quote(args[args.index('--request')+1])+'; nsh /travail/r2-'+kind+'-confirmer.nsh'
        raise ValueError('script non préprovisionné')
    raise ValueError('mode inconnu')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('arch', choices=['aarch64', 'x86_64'])
    parser.add_argument('sector', type=int, choices=[512, 4096])
    parser.add_argument('--issue-vfs-perdue',action='store_true',help='Q2 perte de reponse FINISH durable, consultation sans seconde E/S')
    parser.add_argument('--vfs-en-vol',action='store_true',help='E/S VFS réelle avant retrait, FINISH individuel après réservation')
    parser.add_argument('--membership-io',action='store_true',help='Q1 droits VFS Direct/G/H réels')
    parser.add_argument('--lot', choices=['lectures', 'versions', 'groupe', 'dernier-administrateur', 'desactivation', 'vue-groupe', 'renommage', 'appartenance', 'documentation', 'creation', 'refus-facteurs', 'facteurs-concurrents','retrait-selectif-vide'], required=True)
    parser.add_argument('--mode-commande', choices=['interactif','nsh-c','script'], default='interactif')
    parser.add_argument('--commande', choices=['whoami', 'id', 'groups', 'useradd', 'usermod', 'groupadd'])
    parser.add_argument('--redemarrage-issue', action='store_true', help='mutation sans retrait, capture native, arrêt complet et consultation après reprise approuvée')
    parser.add_argument('--coupure-avant-publication',action='store_true',help='arrêt QEMU avant admission de la mutation ; reprise sans résultat inventé')
    parser.add_argument('--carol-reprise',type=lambda v:Path(v).resolve(),help='artefacts privés du signataire Carol, utilisés seulement après création native')
    parser.add_argument('--course',action='store_true',help='ticket Bob présenté pendant la confirmation Alice')
    parser.add_argument('--mutation-concurrente',action='store_true',help='confirmation de groupe concurrente au refus du dernier administrateur')
    parser.add_argument('--dernier-concurrent',action='store_true',help='deux requêtes partielles simultanées contre le dernier administrateur')
    parser.add_argument('--bob-minimal',action='store_true',help='Bob authentifié puis fermé, contrôles négatifs séparés')
    parser.add_argument('--etat-ticket',choices=['expire','recu','concurrent'],help='état EXEC_TICKET S6 distinct du témoin révoqué')
    parser.add_argument('--separation',action='store_true',help='références réelles Alice/Bob, sans transfert de droits')
    parser.add_argument('--reactivation',action='store_true',help='recette S9 après retrait effectif')
    parser.add_argument('--securite',action='store_true',help='sonde directe étendue, distincte du témoin temporel')
    parser.add_argument('--ticket-vivant',action='store_true',help='désactivation pendant que Bob conserve un ticket non consommé')
    parser.add_argument('--reauth-issue', action='store_true',
                        help='authentification explicite neuve après publication, avant consultation de son issue')
    parser.add_argument('--acquittement-selectif-absent',action='store_true',help='coupure avant admission du premier ACK, maintien UNCERTAIN sans ACK inventé')
    parser.add_argument('--publication-selective-perdue',action='store_true',help='perte de réponse après publication durable, puis réconciliation sans seconde publication')
    parser.add_argument('--acquittement-selectif-perdu',action='store_true',help='perte unique du premier ACK durable VFS, puis QUERY sans réémission')
    parser.add_argument('--recu-supprime', action='store_true',
                        help='supprimer une fois le reçu après publication durable ; conserver UNCERTAIN')
    for name in ('iso', 'registre', 'genese', 'cles', 'decision', 'secrets',
                 'sortie', 'detenteur', 'capture', 'triplet', 'kernel'):
        parser.add_argument('--' + name, type=lambda v: Path(v).resolve(), required=True)
    a = parser.parse_args()
    from certificats import validity
    certificate_dates = validity(a.cles)
    if a.separation and (a.ticket_vivant or a.reactivation or a.securite):parser.error('profils de recette distincts')
    if a.mutation_concurrente and a.lot!='groupe':parser.error("mutation concurrente : lot groupe requis")
    a.dernier_concurrent=a.dernier_concurrent or a.mutation_concurrente
    a.bob_minimal=a.bob_minimal or a.dernier_concurrent
    a.ticket_vivant = a.ticket_vivant or a.reactivation or a.course
    atomic_status = a.ticket_vivant or a.separation or bool(a.etat_ticket) or a.bob_minimal
    if a.etat_ticket and (a.separation or a.ticket_vivant or a.securite):parser.error("profils de recette distincts")
    if a.securite and a.ticket_vivant:parser.error('les deux profils sont distincts')
    if a.ticket_vivant and a.lot!='desactivation':
        parser.error('le témoin de ticket vivant exige le lot désactivation')
    if (a.reauth_issue or a.recu_supprime) and a.lot not in ('groupe','desactivation','creation','renommage','appartenance'):
        parser.error('options de publication réservées aux lots de mutation')
    if a.redemarrage_issue and (a.lot not in ('creation','groupe','renommage','appartenance','retrait-selectif-vide') or a.reauth_issue):parser.error('reprise réservée aux mutations nominales sans retrait avec capture native')
    if a.carol_reprise and (a.lot!='creation' or not a.redemarrage_issue):parser.error('Carol exige création et reprise')
    if a.coupure_avant_publication and (a.lot!='creation' or not a.redemarrage_issue or a.recu_supprime or a.carol_reprise):parser.error('coupure isolée avant création')
    a.membership_r2=a.lot=='retrait-selectif-vide'
    if a.vfs_en_vol and (not a.membership_io or a.issue_vfs_perdue):parser.error('admission en vol distincte de perte de FINISH, VFS réel requis')
    if a.issue_vfs_perdue and not a.membership_io:parser.error('perte issue exige VFS réel')
    if a.membership_io and not a.membership_r2:parser.error('droits réels réservés au retrait sélectif')
    if a.publication_selective_perdue and (not a.membership_r2 or a.acquittement_selectif_perdu):parser.error('perte de publication sélective isolée requise')
    if a.acquittement_selectif_perdu and not a.membership_r2:parser.error('perte ACK réservée au retrait sélectif')
    if a.acquittement_selectif_absent and (not a.membership_r2 or a.acquittement_selectif_perdu or a.publication_selective_perdue):parser.error('absence ACK sélectif isolée requise')
    a.capture_administrative = a.redemarrage_issue
    carol_active=False
    a.mode = 'nominal'
    a.vue = a.activation = a.exec_admission = a.administration_r2 = True
    a.collecteur_r2 = True
    a.vfs = a.mutation = False
    a.lecture_privee = 'autorisee'
    a.transport_connection_limit = 128
    a.legacy_disk = a.genese.with_name('autorite.raw')
    a.sortie.mkdir(parents=True, exist_ok=False)
    a.secrets.mkdir(parents=True, exist_ok=False, mode=0o700)
    root = a.sortie
    report = {'succes': False, 'architecture': a.arch, 'secteur': a.sector,
              'portee': 'création de groupe' if a.lot == 'groupe' else
                        'consultations ou versions des ELF de mutation ; aucune mutation', 'lot': a.lot,
              'mode_commande':a.mode_commande, 'commandes': [], 'drapeaux_locale': 0, 'langue_ELF': 'en'}
    raw = a.triplet.read_bytes()
    size = int.from_bytes(raw[152:156], 'big')
    users = json.loads(raw[156:156 + size])
    subjects = tuple((bytes(u['id']), u['authentication']['epoch'])
                     for u in users if u['status'] == 'Active')
    ids = a.genese.with_name('identites-temoins.bin').read_bytes()
    if len(ids) != 48:
        raise Refus('identités témoins invalides')
    alice, bob, group = (ids[i:i+16].hex() for i in (0, 16, 32))
    membership_fixture=None
    if a.membership_io:
        membership_fixture=a.genese.with_name('membership.bin').read_bytes()
        if len(membership_fixture)!=72 or membership_fixture[8:24].hex()!=group:raise Refus('fixture Q1 mal liée')
        report['fixture_q1_sha256']=hashlib.sha256(membership_fixture).hexdigest()
    case_status = {}
    catalog=json.loads((Path(__file__).parent/'references-administration-r2/catalogue-public.json').read_text())
    cases = [('whoami', b'alice\n'),
             ('id', f'subject={alice} name=alice state=ACTIVE groups=base({group})\n'.encode()),
             ('groups', b'base\n')]
    cases += [(n + ' --version', (n + ' 0.24.0 NovaOS NXADMIN1 experimental\n').encode())
              for n in ('useradd', 'usermod', 'groupadd')]
    # Deux parcours indépendants : le total de connexions du relais hôte reste
    # borné à 128 et chaque parcours commence avec une nouvelle session.
    # L'essai initial de six commandes ayant saturé ce relais est conservé.
    report['validite_certificats_hote'] = certificate_dates
    cases = cases[:3] if a.lot == 'lectures' else cases[3:]
    if a.lot in ('groupe','desactivation','renommage','appartenance','creation','retrait-selectif-vide'):
        intention = {}

        def preparation(data):
            match = re.fullmatch(rb'request=([0-9a-f]{32}) confirmation=([0-9a-f]{64}) state=PREPARED\n', data)
            if not match:
                raise Refus('préparation non canonique')
            intention.update(operation=match[1].decode(), confirmation=match[2].decode())
            report['intention'] = dict(intention)
            return data

        def complete(_):
            return ('request=' + intention['operation'] + ' generation='+str(int.from_bytes(raw[96:104],'big')+1)+' state=COMPLETE\n').encode()

        command={'retrait-selectif-vide':'usermod --remove-group base alice','groupe':'groupadd equipe-r2','desactivation':'usermod --disable bob','renommage':'usermod --rename robert bob','appartenance':'usermod --add-group equipe-r2 bob','creation':'useradd --factor-reference '+catalog['references']['carol']+' --group base carol'}[a.lot]
        binary=command.split()[0]
        report['portee']='création de groupe' if a.lot=='groupe' else 'désactivation de Bob après fermeture de sa session ; aucun témoin de ticket vivant'
        if a.ticket_vivant:report['portee']='désactivation de Bob avec ticket vivant ; refus avant expiration exigé'
        cases = [(command+' --prepare', preparation),
                 (lambda: command+' --confirm ' + intention['confirmation'] +
                  ' --request ' + intention['operation'], complete),
                 (lambda: binary+' --outcome ' + intention['operation'], complete)]
        def group_created(_):
            generation=int.from_bytes(raw[96:104],'big')+1
            identity=hashlib.sha256(b'NovaAuth/subject/1'+raw[8:24]+generation.to_bytes(8,'little')+bytes.fromhex(intention['operation'])+b'\x02').digest()[:16].hex()
            return f'group={identity} name=equipe-r2 members=\n'.encode()
        if a.membership_r2:
            report['portee']='Droits VFS Direct/G/H réels et injection déclarée ; EXEC G, Q2/Q3 et reprise froide non qualifiés' if a.membership_io else 'raccord administratif sans delegation G ouverte ; ne qualifie pas Q1-Q4'
        if a.lot == 'renommage':
            report['portee']='renommage natif, identité stable, aucun changement des droits'
            cases[-1]=('id --subject-id '+bob,f'subject={bob} name=robert state=ACTIVE groups=base({group})\n'.encode())
        if a.lot == 'appartenance':
            report['portee']='ajout natif à un groupe existant ; aucun retrait de droit'
            cases[-1]=('groups bob',b'base equipe-r2\n')
        if a.lot == 'creation':
            report['portee']='création native avec facteur préprovisionné ; aucune promotion administrative'
            def created_identity(_):
                generation=int.from_bytes(raw[96:104],'big')+1
                subject=hashlib.sha256(b'NovaAuth/subject/1'+raw[8:24]+generation.to_bytes(8,'little')+bytes.fromhex(intention['operation'])+b'\x01').digest()[:16].hex()
                report['nouvelle_identite']=subject
                return f'subject={subject} name=carol state=ACTIVE groups=base({group})\n'.encode()
            cases[-1]=('id carol',created_identity)
        if a.reactivation:
            second={}
            def prepare_reactivation(data):
                match=re.fullmatch(rb'request=([0-9a-f]{32}) confirmation=([0-9a-f]{64}) state=PREPARED\n',data)
                if not match:raise Refus('préparation de réactivation non canonique')
                second.update(operation=match[1].decode(),confirmation=match[2].decode())
                report['reactivation']=dict(second)
                return data
            def reactivated(_):
                return ('request='+second['operation']+' generation=25 state=COMPLETE\n').encode()
            # Les deux confirmations portent déjà leur reçu durable. Les
            # consultations Outcome appartiennent à S8 ; ne pas les répéter
            # dans S9 et saturer la borne inchangée de 128 connexions du banc.
            cases=cases[:2]
            cases += [('usermod --enable bob --prepare',prepare_reactivation),
                      (lambda:'usermod --enable bob --confirm '+second['confirmation']+' --request '+second['operation'],reactivated)]
    elif a.lot == 'refus-facteurs':
        report['portee']='facteur inconnu et déjà attribué refusés ; contrôle positif Alice'
        cases=[('useradd --factor-reference '+('ff'*16)+' --group base inconnu --prepare',b''),
               ('useradd --factor-reference '+catalog['references']['alice']+' --group base doublon --prepare',b''),('whoami',b'alice\n')]
        case_status={0:(3,b'Administrative failure: native=5 status=3\n'),1:(4,b'Administrative failure: native=24 status=4\n')}
    elif a.lot == 'documentation':
        expected=(Path(__file__).parent/'references-administration-r2')
        cases=[('LANG=fr; groups man',(expected/'groups-fr.txt').read_bytes()),
               ('LANG=en; man groups',(expected/'groups-en.txt').read_bytes())]
        report['portee']='LANG=fr/en et man générique : flux de documentation exacts'
    elif a.lot == 'vue-groupe':
        report['portee']='consultation native du groupe et de ses membres autorisés'
        expected=f'group={group} name=base members={alice} {bob}\n'.encode()
        cases=[('groups --group base',expected),('groups --group-id '+group,expected)]
    elif a.lot == 'dernier-administrateur':
        cases = [('usermod --disable alice --prepare', b'')]
    if a.separation:
        cases=[('ticket-test separation whoami',b'alice\n')]
        report['portee']='S3/S5 références réelles sans délégation, session propre Bob et lancement Alice'
    if a.etat_ticket:
        cases=[('ticket-test '+a.etat_ticket+' whoami',b'' if a.etat_ticket=='expire' else b'alice\n')]
        if a.etat_ticket=='expire':cases.append(('whoami',b'alice\n'))
        report['portee']='S6 ticket '+a.etat_ticket+' et contrôle positif Alice'
    if a.dernier_concurrent:
        cases=[('admin-sonde --last-admin-writer | admin-sonde --last-admin-reader',(5).to_bytes(8,'little')+bytes(56)),('whoami',b'alice\n')]
        report['portee']='S7 deux préparations concurrentes du retrait du dernier administrateur, puis lecture Alice'
        if a.mutation_concurrente:
            def joined_receipt():
                return bytes(8)+bytes.fromhex(intention['operation'])+(24).to_bytes(8,'little')+bytes([1])+bytes(31)+(5).to_bytes(8,'little')+bytes(56)
            cases=[('admin-sonde --prepare-group',preparation),
                   (lambda:'admin-sonde --confirm-writer '+intention['operation']+' '+intention['confirmation']+' | admin-sonde --last-admin-reader-with-mutation',joined_receipt),
                   ('whoami',b'alice\n')]
            report['portee']='S7 publication explicite de groupe concurrente au refus du retrait du dernier administrateur'

    if a.lot=='facteurs-concurrents':
        a.dernier_concurrent=True
        factor=catalog['references']['carol']
        def factor_results(data):
            if len(data)!=160:raise Refus('deux résultats de création absents')
            rows=[]
            for offset in (0,80):
                p,c=int.from_bytes(data[offset:offset+8],'little'),int.from_bytes(data[offset+8:offset+16],'little')
                if p not in (0,24) or (p==0 and c not in (0,5,10,24)) or (p!=0 and c!=2**64-1):raise Refus('statuts concurrence facteur inattendus')
                rows.append({'preparation':p,'confirmation':c,'recu':data[offset+16:offset+80].hex()})
            if sum(r['preparation']==0 and r['confirmation']==0 for r in rows)!=1:raise Refus('création unique non démontrée')
            report['attribution_concurrente']=rows
            return data
        cases=[('admin-sonde --factor-writer '+factor+' | admin-sonde --factor-reader '+factor,factor_results),('whoami',b'alice\n')]
        report['portee']='deux demandes PREPARE simultanément en vol, facteur commun, utilisateurs distincts'

    if a.redemarrage_issue and a.membership_r2:
        cases=cases[:2] # la consultation est exercée après arrêt complet
    if a.commande:
        cases = [(cmd, expected) for cmd, expected in cases if isinstance(cmd, str) and cmd.split()[0] == a.commande]
        if not cases:
            parser.error('commande absente du lot demandé')
    with ExitStack() as stack:
        env = fixture(a, stack)
        dispatcher, holders, admin = env
        api = execs.Admissions(dispatcher.authority.authority)
        execs.initialize(api.path)
        initialize(api.path)
        admin.call('r2-subjects', subjects)
        old = route(api)
        api.enroll(SERVER, DOMAIN, old, signature(admin, 'enroll', old))
        api.begin_withdrawal(SERVER, DOMAIN, old, signature(admin, 'withdraw', old))
        api.drain(SERVER, DOMAIN, old)
        enrolled = False
        discarded_receipt = False
        cut_before_publication=False
        selective_calls={}
        restarted=False

        def rpc_hook(phase, operation, body, result, qmp, disk):
            nonlocal enrolled, discarded_receipt, cut_before_publication
            if a.membership_r2 and phase=='before' and 54<=operation<=61:
                selective_calls[operation]=selective_calls.get(operation,0)+1
            if phase=='after' and operation==58 and a.issue_vfs_perdue and not discarded_receipt:
                with transaction(api.path) as db:
                    rows=db.execute('SELECT op,state,evidence FROM membership_operations ORDER BY sequence').fetchall()
                if len(rows)!=1 or rows[0][1]!=1 or rows[0][2] is None:raise Refus('fait VFS non durable avant perte')
                report['issue_vfs_perdue']={'operation':rows[0][0].hex(),'etat':1,'fait':rows[0][2].hex(),'durable_avant_perte':True}
                discarded_receipt=True
                raise Refus('UNCERTAIN : reponse FINISH VFS perdue apres persistance')
            if phase=='before' and operation==50 and a.coupure_avant_publication and not cut_before_publication:
                from exec_vm_debug import qmp as control_vm
                with transaction(api.path) as db:
                    if db.execute('SELECT count(*) FROM admin_changes').fetchone()!=(0,):raise Refus('mutation déjà présente avant injection')
                    state=api.authority._current(db,SERVER,DOMAIN)
                if state.generation!=int.from_bytes(raw[96:104],'big'):raise Refus('tête déjà modifiée avant coupure')
                cut_before_publication=True
                report['coupure_avant_publication']={'operation':intention['operation'],'generation':state.generation,'mutation_absente':True,'statut_confirmation':None,'arret_force_QEMU':True}
                control_vm(qmp,'quit')
                raise Refus('coupure injectée avant admission durable, aucune confirmation')
            if phase=='before' and operation==56 and a.acquittement_selectif_absent and not discarded_receipt:
                with transaction(api.path) as db:
                    rows=db.execute('SELECT acknowledged FROM membership_effectors WHERE op=? ORDER BY kind',
                        (bytes.fromhex(intention['operation']),)).fetchall()
                if rows!=[(0,),(0,),(0,)]:raise Refus('acquittement déjà admis avant coupure')
                discarded_receipt=True
                report['acquittement_selectif_absent']={'classe':1,'durable':False,'admission_interrompue':True}
                raise Refus('UNCERTAIN : émission ACK interrompue avant admission')
            if phase=='after' and operation==55 and a.publication_selective_perdue and not discarded_receipt:
                with transaction(api.path) as db:
                    row=db.execute('SELECT state FROM membership_changes WHERE op=?',
                        (bytes.fromhex(intention['operation']),)).fetchone()
                    mutations=db.execute('SELECT count(*) FROM mutations').fetchone()[0]
                if row!=(1,) or mutations!=1:raise Refus('publication durable non démontrée avant perte')
                discarded_receipt=True
                report['publication_selective_perdue']={'etat':1,'mutations':1,'durable_avant_perte':True}
                raise Refus('UNCERTAIN : réponse de publication sélective perdue après persistance')
            if phase=='after' and operation==56 and a.acquittement_selectif_perdu and not discarded_receipt:
                with transaction(api.path) as db:
                    rows=db.execute('SELECT kind,acknowledged FROM membership_effectors WHERE op=? ORDER BY kind',
                        (bytes.fromhex(intention['operation']),)).fetchall()
                if rows!=[(1,1),(2,0),(4,0)]:raise Refus('ACK VFS durable non démontré avant injection')
                discarded_receipt=True
                report['acquittement_selectif_perdu']={'classe':1,'durable_avant_perte':True}
                raise Refus('UNCERTAIN : réponse ACK sélectif perdue après admission durable')
            if phase == 'after' and operation == 50 and a.recu_supprime and not discarded_receipt:
                discarded_receipt = True
                # L'autorité a déjà décidé. Le banc supprime uniquement le reçu,
                # jamais la mutation, et ne soumet aucune seconde demande.
                report['recu_supprime_apres_publication'] = True
                raise Refus('UNCERTAIN : reçu administratif supprimé par le banc')
            if phase == 'before' and (operation == 40 or (a.membership_io and operation == 57)) and not enrolled:
                with transaction(api.path) as db:
                    state = api.authority._current(db, SERVER, DOMAIN)
                proof = admin.call('r2-enroll', state)
                receipt, sig = Epochs(api.authority).enroll(state, subjects, proof)
                (root / 'enrolement-epoques.bin').write_bytes(receipt + sig)
                enrolled = True

        def scenario(serial, control, disk, out):
            from administration_collector_qemu import Observer
            observer = Observer(control, a.arch, a.kernel, out)
            stack.callback(observer.close)
            events = Events.before_ready(serial, rb'NAEXEC registre pret puis connexion du vrai nsh', 100)
            report['evenements'] = events.records
            # Ticket vivant : préparer Alice dès que le lanceur est prêt,
            # avant le déclenchement explicite de l'authentification de Bob.
            # Les contrôles négatifs sont un parcours distinct.
            # Les autres lots conservent leur ordonnancement historique.
            if a.separation:
                serial.expect(rb'NAR2 Bob references pret',40)
            elif a.etat_ticket or a.bob_minimal:
                serial.expect(rb'NAR2 Bob fermeture acquittee',40)
                if a.dernier_concurrent:
                    pid=events.actor(b'NAR2 Bob authentifie route distincte')
                    serial.expect(rb'NAEXEC restitue pid='+str(pid).encode()+rb' roots=0 handles=0 waits=0 pages=0 pile=0 threads=0',20)
            elif not a.ticket_vivant:
                serial.expect(rb'NAR2 Bob fermeture acquittee', 100)
                if b'NAR2 Bob refus serveur et substitutions verifies' not in serial.data:
                    raise Refus('témoin négatif Bob absent')
            if carol_active:
                pid=events.actor(b'NAR2 Bob authentifie route distincte')
                found=re.findall(rb'DEBUG pid='+str(pid).encode()+rb' R2IDENTITY subject= ([0-9a-f]{32})',serial.data)
                if found!=[report['nouvelle_identite'].encode()]:raise Refus('Carol non authentifiée avec son identité native')
                if b'NAR2 Carol sans installations heritees statut=5 aucun enfant' not in serial.data:raise Refus('refus des installations héritées absent')
                report['authentification_carol']={'pid':pid,'subject':found[0].decode(),'facteur_public':catalog['references']['carol'],'installations_refusees':['whoami','admin-sonde','alice-seulement'],'statuts':[5,5,5]}
            def membership_probe(phase):
                f=membership_fixture
                command='membership-test '+phase+' '+str(int.from_bytes(f[:8],'little'))+' '+f[8:24].hex()+' '+f[24:40].hex()
                start=len(serial.data);serial.offset=start
                serial.submit(command+'; echo NXQ1_'+phase+'_$?__FIN')
                revisions={}
                before=phase in ('avant','en-vol')
                for origin in (('Direct','H','G') if phase=='en-vol' else ('Direct','G','H')):
                    code=10 if not before and origin=='G' else 0
                    matched=serial.expect(membership_revision_pattern(phase,origin,code),60)
                    revisions[origin]=int(matched[1])
                if before:
                    order=('Direct','H','G') if phase=='en-vol' else ('Direct','G','H')
                    if not revisions[order[0]]<revisions[order[1]]<revisions[order[2]]:raise Refus('écritures G/H non démontrées par les révisions P9')
                else:
                    before=report['droits_vfs_reels'][-1]['revisions_P9']
                    if revisions['Direct']!=before['G' if phase=='apres-en-vol' else 'H'] or revisions['G']!=0 or revisions['H']<=revisions['Direct']:raise Refus('écriture G non refusée ou H non conservée')
                status=serial.expect(('NXQ1_'+phase+r'_(\d+)__FIN').encode(),20)
                if int(status[1])!=0:raise Refus('lecture VFS Q1 échouée')
                (out/('q1-'+phase+'.bin')).write_bytes(bytes(serial.data[start:serial.offset]))
                report.setdefault('droits_vfs_reels',[]).append({'phase':phase,'Direct':0,'G':0 if phase in ('avant','en-vol') else 10,'H':0,'reference_hex_par_origine':{origin:((bytes([first])+b'\tB\tC\nD\tE\tF\n').hex() if first is not None else '') for origin,first in [('Direct',65 if phase in ('avant','en-vol') else f[8 if phase=='apres-en-vol' else 24]^(0x80 if phase=='reprise' else 0)),('G',f[8] if phase in ('avant','en-vol') else None),('H',f[24]^(0x80 if phase.startswith('apres') else 0))]},'comparaison_invitee':True,'revisions_P9':revisions,'droits_demandes':{'Direct':1,'G':3,'H':3}})
            if a.membership_io:membership_probe('en-vol' if a.vfs_en_vol else 'avant')
            if a.vfs_en_vol:
                with transaction(api.path) as db:
                    live=db.execute('SELECT grp,state FROM membership_operations ORDER BY sequence').fetchall()
                if live!=[(membership_fixture[24:40],1),(membership_fixture[8:24],0)]:raise Refus('admission G non réellement en vol avant retrait')
                report['vfs_G_en_vol_avant_retrait']=True
            for i, (command, expected) in enumerate(cases):
                expected_status = 3 if a.lot == 'dernier-administrateur' else 0
                expected_stderr = b''
                if a.lot == 'dernier-administrateur':
                    expected_stderr = b'Administrative failure: native=5 status=3\n'
                elif ((a.recu_supprime or a.acquittement_selectif_perdu or a.publication_selective_perdue) and i == 1) or (a.acquittement_selectif_absent and i in (1,2)):
                    expected_status = 20
                    expected = b''
                    expected_stderr = ('Administrative failure: native=22 status=20\nrequest=' +
                                       intention['operation'] + ' --outcome\n').encode()
                if i in case_status:
                    expected_status,expected_stderr=case_status[i]
                if a.reauth_issue and i == 2:
                    observer.expect_successor(report['commandes'][0]['shell'])
                    serial.submit('session reauthentifier')
                    serial.expect('NovaAuth : nouvelle session authentifiée'.encode(), 40)
                    serial.expect(rb'nsh> ', 20)
                    report['nouvelle_authentification_explicite_avant_issue'] = True
                if a.reactivation and i==3:
                    observer.expect_successor(events.actor(b'NAR2 Bob attente declenchement authentification'))
                if callable(command):
                    command = command()
                command = command_mode(command, a.mode_commande)
                # Identité d'étape distincte ; aucune réponse d'une commande
                # précédente ne peut terminer la commande courante.
                step = 'NXR2_' + str(i) + '_'
                line=command + '; echo ' + step + '$?__FIN' + ('; exit' if i == len(cases)-1 and not (a.reactivation or a.separation or a.membership_io or (a.membership_r2 and restarted)) else '')
                serial.offset = len(serial.data)
                start = serial.offset
                serial.submit(line)
                pattern=observed_command_status_pattern(step) if atomic_status else command_status_pattern(step)
                status = serial.expect(pattern, 80)
                shell_status=int(status[2] if atomic_status else status[1])
                # Le marqueur DEBUG atomique identifie la fin et le statut.
                # Le prompt CONSOLE peut être entrelacé avec Bob sur le port série.
                if not atomic_status and i != len(cases)-1:
                    serial.expect(rb'nsh> ', 20)
                output = bytes(serial.data[start:serial.offset])
                (out / f'commande-{i:02}.bin').write_bytes(output)
                pids = set(int(p) for p in re.findall(rb'NXFLUX pid=(\d+) fd=', output))
                if a.ticket_vivant:
                    # Identifier les enfants par les liaisons noyau collectées,
                    # indépendamment des sorties Bob reçues en parallèle.
                    bob_pid = events.actor(b'NAR2 Bob attente declenchement authentification')
                    with observer.guard:
                        alice_routes = [r for r in observer.routes if r[0] != bob_pid]
                    if len(alice_routes) != 1:
                        raise Refus('route Alice absente ou ambiguë')
                    alice_pid = alice_routes[0][0]
                    pids &= {binding.child for binding, _, _ in observer.outputs()
                             if binding.shell == alice_pid}
                item = {'commande': command, 'statut_shell': shell_status,
                        'statut_attendu': expected_status, 'pid': sorted(pids)}
                report['commandes'].append(item)
                if a.dernier_concurrent and i==(1 if a.mutation_concurrente else 0):
                    if a.mutation_concurrente:expected=expected()
                    match=re.search(rb'NXPIPE producteur=(\d+) statut=0 lecteur=(\d+) statut=0',output)
                    if shell_status!=0 or not match or match[1]==match[2]:raise Refus('deux PID/statuts du pipe absents')
                    writer,reader=map(int,match.groups());item['pid']=[writer,reader]
                    rows=[(binding,closed,data) for binding,closed,data in observer.outputs() if binding.child in (writer,reader)]
                    actual={(b.child,b.fd):data for b,_,data in rows}
                    if a.lot=='facteurs-concurrents':expected=expected(actual.get((reader,1),b''))
                    if len(rows)!=3 or any(not closed for _,closed,_ in rows) or actual!={(writer,2):b'',(reader,1):expected,(reader,2):b''}:
                        raise Refus('refus natifs concurrent dernier administrateur non conformes')
                    shells={b.shell for b,_,_ in rows}
                    if len(shells)!=1 or int(status[1]) not in shells:raise Refus('pipe hors session Alice')
                    item.update(shell=next(iter(shells)),stdout_octets=len(expected),stderr_octets=0,octets_conformes=True,statuts_requetes=report.get('attribution_concurrente',[0,5] if a.mutation_concurrente else [5,5]))
                    (out/'concurrence-refus.bin').write_bytes(expected)
                    continue
                no_child=a.etat_ticket=='expire' and i==0
                if shell_status != expected_status or len(pids) != (0 if no_child else 1):
                    raise Refus('statut ou PID non conforme : ' + command)
                if no_child:
                    if b'NXEXEC ticket expire, aucun enfant' not in output:raise Refus('refus natif expiration absent')
                    item.update(statut_ticket=8,stdout_octets=0,stderr_octets=0,octets_conformes=True)
                    continue
                pid = next(iter(pids))
                rows = [(binding, closed, data) for binding, closed, data in observer.outputs()
                        if binding.child == pid]
                if len(rows) != 2 or any(not closed for _, closed, _ in rows):
                    raise Refus('sorties non drainées : ' + command)
                streams = {b.fd: data for b, _, data in rows}
                shells = {b.shell for b, _, _ in rows}
                if len(shells) != 1:
                    raise Refus('sorties rattachées à plusieurs shells')
                item['shell'] = next(iter(shells))
                if atomic_status and int(status[1]) != item['shell']:
                    raise Refus('statut provenant d’une autre instance')
                for fd, data in streams.items():
                    (out / f'commande-{i:02}-fd{fd}.bin').write_bytes(data)
                if callable(expected):
                    expected = expected(streams.get(1, b''))
                if streams != {1: expected, 2: expected_stderr}:
                    raise Refus('octets non conformes : ' + command)
                item['stdout_octets'] = len(expected)
                item['stderr_octets'] = len(expected_stderr)
                match_native = re.search(rb'native=([0-9]+)', streams[2])
                if match_native: item['statut_novaAuth'] = int(match_native[1])
                item['octets_conformes'] = True
                if a.ticket_vivant and i==0:
                    # Authentifier Bob hors de l'attente NXADMIN1 de l'ELF de
                    # confirmation. Le ticket n'existe pas encore à ce point.
                    bob_pid=events.actor(b'NAR2 Bob attente declenchement authentification')
                    events.expect(bob_pid,b'NAR2 Bob authentifie route distincte',40)
                    events.expect(bob_pid,('NAR2 Bob declenchement operation='+intention['operation']).encode(),40)
                if a.ticket_vivant and i==1:
                    bob_pid=events.actor(b'NAR2 Bob attente declenchement authentification')
                    events.expect(bob_pid,b'NAR2 Bob ticket vivant non consomme',40)
                    with observer.guard: routes=list(observer.routes)
                    selected={r[0]:r for r in routes}
                    if len(routes)!=2 or set(selected)!={alice_pid,bob_pid}:
                        raise Refus('deux routes authentifiées distinctes absentes')
                    report['sessions_temoin']={name:{'pid':pid,'instance':selected[pid][1].hex(),'session':selected[pid][2].hex()}
                        for name,pid in [('Alice',alice_pid),('Bob',bob_pid)]}
                    for record in events.records:
                        route=selected[record['pid']]
                        record.update(instance=route[1].hex(),session=route[2].hex())

                if a.reactivation and i==3:
                    bob_pid=events.actor(b'NAR2 Bob attente declenchement authentification')
                    for message in [b'NAR2 Bob nouvelle session explicite distincte',
                                    b'NAR2 Bob apres reactivation ancien ticket=10 ancienne session=5',
                                    b'NAR2 Bob nouveau lancement autorise droits ordinaires',
                                    b'NAR2 Bob fermeture acquittee']:
                        events.expect(bob_pid,message,40)
            if a.membership_io:
                if b'NARM VFS fermeture selective acquittee retires=01 conserves=02' not in serial.data:raise Refus('fermeture réelle du seul handle G non démontrée')
                membership_probe('apres-en-vol' if a.vfs_en_vol else 'apres')
            if a.membership_r2 and not restarted:
                terminal=b'NARM RETIREE recu durable tete P9 relue effecteurs acquittes'
                if a.acquittement_selectif_absent and terminal in serial.data:raise Refus('RETIREE inventé sans ACK durable')
                markers=(b'NARM EXEC inspection instance et provenance personnelle acquittee',b'NARM VFS fermeture selective acquittee')+(() if a.acquittement_selectif_absent else (terminal,))
                for marker in markers:
                    if marker not in serial.data:raise Refus('acquittement selectif invite absent')
                with transaction(api.path) as db:
                    row=db.execute('SELECT state,generation,subject,grp FROM membership_changes WHERE op=?',(bytes.fromhex(intention['operation']),)).fetchone()
                    effects=db.execute('SELECT kind,instance,acknowledged FROM membership_effectors WHERE op=? ORDER BY kind',(bytes.fromhex(intention['operation']),)).fetchall()
                if row is None or row[0]!=(1 if a.acquittement_selectif_absent else 2) or row[2].hex()!=alice or row[3].hex()!=group or len(effects)!=3 or [k for k,_,v in effects if v==1]!=([] if a.acquittement_selectif_absent else [1,2,4]):raise Refus('retrait durable ou effecteurs incomplets')
                expected_calls=[(54,1),(55,1),(56,1)] if a.acquittement_selectif_absent else [(54,1),(55,1),(56,3)]
                if any(selective_calls.get(k,0)!=v for k,v in expected_calls):raise Refus('mutation ou acquittement sélectif répété')
                if (a.acquittement_selectif_perdu or a.publication_selective_perdue or a.acquittement_selectif_absent) and not discarded_receipt:raise Refus('perte non exercée')
                with transaction(api.path) as db:
                    op=bytes.fromhex(intention['operation'])
                    if db.execute('SELECT count(*) FROM mutations WHERE op=?',(op,)).fetchone()!=(1,):raise Refus('publication non unique')
                    ordered=db.execute("SELECT sequence,kind FROM exec_events WHERE kind LIKE 'membership-%' ORDER BY sequence").fetchall()
                expected_order=['membership-reserved','membership-withdrawing']+([] if a.acquittement_selectif_absent else ['membership-effector-ack']*3+['membership-retired'])
                if [k for _,k in ordered if k not in ('membership-admitted','membership-operation-finished')]!=expected_order:raise Refus('ordre durable sélectif non conforme')
                report['rpc_selectifs']=selective_calls
                report['ordre_durable']=[{'position':n,'transition':k} for n,k in ordered]
                report['retrait_selectif']={'etat':'RETRAIT_EN_COURS' if a.acquittement_selectif_absent else 'RETIREE','generation':row[1],'effecteurs':[{'classe':k,'instance':i.hex(),'acquitte':bool(v)} for k,i,v in effects],'delegations_G_ouvertes':a.membership_io}
                report['injection_vfs_reelle']=('ack-absent' if a.acquittement_selectif_absent else 'ack-perdu' if a.acquittement_selectif_perdu else 'publication-perdue' if a.publication_selective_perdue else 'aucune') if a.membership_io else None
                if a.membership_io:
                    with transaction(api.path) as db:
                        admissions=db.execute('SELECT op,subject,grp,kind,instance,packet,sequence,state,evidence,terminal_sequence FROM membership_operations ORDER BY sequence').fetchall()
                    if len(admissions)!=3 or any(r[7]!=1 or r[8] is None or r[9]<=r[6] for r in admissions):raise Refus('issues individuelles Q1 incomplètes')
                    if [r[2] for r in admissions]!=([membership_fixture[24:40],membership_fixture[8:24],membership_fixture[24:40]] if a.vfs_en_vol else [membership_fixture[8:24],membership_fixture[24:40],membership_fixture[24:40]]):raise Refus('origines Q1 différentes')
                    if a.vfs_en_vol:
                        reserve=next(n for n,k in ordered if k=='membership-reserved')
                        publish=next(n for n,k in ordered if k=='membership-withdrawing')
                        group_op=admissions[1]
                        if not group_op[6]<reserve<group_op[9]<publish:raise Refus('drainage individuel hors ordre durable attendu')
                        if b'NARM Q2 drainage VFS apres reservation avant fermeture' not in serial.data:raise Refus('raccord du drainage au coordinateur absent')
                        report['vfs_drainage_individuel']={'admission':group_op[6],'reservation':reserve,'issue':group_op[9],'publication':publish}
                    if a.issue_vfs_perdue:
                        if not discarded_receipt or selective_calls.get(57)!=3 or selective_calls.get(58)!=3:raise Refus('admission ou FINISH repete apres perte')
                        if b'NARM Q2 UNCERTAIN consultation explicite' not in serial.data or b'NARM Q2 issue individuelle reconciliee sans seconde E/S' not in serial.data:raise Refus('reconciliation invitee non demontree')
                        report['reconciliation_vfs_sans_repetition']=True
                    report['admissions_vfs']=[{'operation':r[0].hex(),'titulaire':r[1].hex(),'groupe':r[2].hex(),'classe':r[3],'effecteur':r[4].hex(),'session':r[5][209:241].hex(),'position':r[6],'etat':r[7],'fait_sha256':r[8].hex(),'terminal':r[9]} for r in admissions]
            if a.membership_r2 and restarted:
                membership_probe('reprise')
                if selective_calls.get(61)!=1:raise Refus('historique sélectif signé non consulté une fois')
                if any(selective_calls.get(k,0)!=v for k,v in [(54,1),(55,1),(56,3)]):raise Refus('mutation ou ACK répété après redémarrage')
                with transaction(api.path) as db:
                    op=bytes.fromhex(intention['operation'])
                    if db.execute('SELECT state FROM membership_changes WHERE op=?',(op,)).fetchone()!=(2,) or db.execute('SELECT count(*) FROM mutations WHERE op=?',(op,)).fetchone()!=(1,):raise Refus('terminal historique absent ou seconde mutation')
                report['recu_selectif_apres_redemarrage']={'rpc':61,'auteur_verifie_dans_invite':True,'recu_RETIREE_verifie_dans_invite':True,'publications':1,'nouvel_ACK':False}
            if a.separation:
                bob_pid=events.actor(b'NAR2 Bob references pret')
                for message in (b'NAR2 S3 ticket Alice statut=5 aucun enfant',b'NAR2 S3 canal Alice statut=5',
                                b'NAR2 S3 session Alice statut=5',b'NAR2 S5 admission session Alice statut=5',
                                b'NAR2 Bob session propre preservee',b'NAR2 Bob fermeture acquittee'):
                    events.expect(bob_pid,message,20)
            if a.reactivation or a.separation or a.membership_io or (a.membership_r2 and restarted):serial.submit('exit')
            serial.expect(rb'BLOC bilan mode=41 service=Exit\(0\) bloc=Exit\(0\) pilote=Exit\(0\) client=Exit\(0\) retrait=Ok\(\(\)\)', 40)
            rows = observer.outputs()
            by_shell = {}
            for binding, closed, data in rows:
                if not closed:
                    raise Refus('flux final ouvert')
                (out / f'flux-{binding.shell}-{binding.child}-{binding.fd}.bin').write_bytes(data)
                by_shell.setdefault(binding.shell, {}).setdefault(binding.child, {})[binding.fd] = data
            bob_oracle = a.genese.with_name('oracle-bob.bin').read_bytes()
            denied = (5).to_bytes(8, 'little') + bytes(56)
            malformed = (3).to_bytes(8, 'little') + bytes(56)
            bob_expected = b''.join([bob_oracle, denied, denied, bob_oracle, denied, malformed, malformed, bob_oracle])
            if a.securite:
                bob_expected += denied*2 + (malformed+denied)*3 + denied*3 + bob_oracle
                report['refus_directs']={'confirm':5,'outcome':5,'append_canal_administratif':[3]*3,
                    'append_sans_capacite':[5]*3,'cibles_substituees':[5]*3,
                    'vue_Bob_inchangee':True,'octets_stdout':len(bob_expected)}
            bob = [s for s, children in by_shell.items() if list(children.values()) == [{1: bob_expected, 2: b''}]]
            if a.ticket_vivant:
                bob = [events.actor(b'NAR2 Bob attente declenchement authentification')]
                if a.reactivation:
                    if len(by_shell)!=2 or list(by_shell.get(bob[0],{}).values())!=[{1:bob_oracle,2:b''}]:
                        raise Refus('lecture Bob après réactivation absente ou privilège modifié')
                    previous=report['sessions_temoin']['Bob']
                    with observer.guard:current=[r for r in observer.routes if r[0]==bob[0]]
                    if len(current)!=1 or current[0][1].hex()!=previous['instance'] or current[0][2].hex()==previous['session']:
                        raise Refus('session Bob réactivée ou instance remplacée')
                    report['nouvelle_session_bob']={'instance':current[0][1].hex(),'session':current[0][2].hex(),
                        'ancien_ticket':10,'ancienne_session':5,'nouveau_lancement':0}
                elif a.course:
                    children=by_shell.get(bob[0],{})
                    if len(children)>1 or any(streams not in ({1:b'A\tB\tC\nD\tE\tF\n',2:b''},{1:b'',2:b''}) for streams in children.values()):
                        raise Refus('sorties de la course Bob non conformes')
                elif bob[0] in by_shell or len(by_shell)!=1:
                    raise Refus('sortie enfant Bob inattendue dans le témoin temporel')
            elif a.etat_ticket or a.bob_minimal:
                bob=[events.actor(b'NAR2 Bob authentifie route distincte')]
                if len(by_shell)!=1 or bob[0] in by_shell:raise Refus('enfant Bob inattendu S6')
                if a.etat_ticket in ('recu','concurrent') and not re.search(rb'NXEXEC pid=([0-9]+) statut=0 collecte unique',serial.data):raise Refus('collecte unique absente')
                if a.etat_ticket:report['etat_ticket']={'mode':a.etat_ticket,'statut_natif':8 if a.etat_ticket=='expire' else 12,'controle_Alice':0}
                if a.etat_ticket=='concurrent':
                    rows=re.findall(rb'NXEXEC concurrence ticket=(\d+) thread=(\d+) statut=(\d+) droit_collecte=(\d+) anciens_spawns_refuses=3/3',serial.data)
                    if len(rows)!=2 or len({r[0] for r in rows})!=1 or len({r[1] for r in rows})!=2 or sorted(int(r[2]) for r in rows)!=[0,12]:
                        raise Refus('consommation concurrente non démontrée')
                    report['concurrence_threads']=[{'thread':int(r[1]),'statut':int(r[2]),'anciens_spawns_refuses':3} for r in rows]
            elif a.separation:
                bob=[events.actor(b'NAR2 Bob references pret')]
                if len(by_shell)!=1 or bob[0] in by_shell or re.search(rb'NAEXEC enfant owner='+str(bob[0]).encode()+rb' child=',serial.data):
                    raise Refus('enfant Bob interdit après références Alice')
                with observer.guard:routes=list(observer.routes)
                if len(routes)!=2 or len({r[1] for r in routes})!=2 or len({r[2] for r in routes})!=2:
                    raise Refus('routes/session non distinctes')
                report['separation']={'ticket_Alice':5,'canal_Alice':5,'session_Alice':5,'admission_session_Alice':5,
                    'enfants_Bob':0,'lancement_Alice':0,'session_Bob_preservee':True,
                    'routes':[{'pid':r[0],'instance':r[1].hex(),'session':r[2].hex()} for r in routes]}
            elif len(by_shell) != 2 or len(bob) != 1:
                raise Refus('sorties Bob confondues ou absentes')
            if a.ticket_vivant:
                if not a.course and b'NAR2 Bob ancien ticket statut=10 aucun enfant' not in serial.data:
                    raise Refus('ancien ticket Bob sans refus REVOKED')
                attempts=re.findall(rb'NAEXEC attente owner='+str(bob[0]).encode()+rb' ticket=(\d+) maintenant=(\d+) echeance=(\d+)',serial.data)
                if not attempts or (not a.reactivation and len(attempts)!=1) or int(attempts[0][1])>=int(attempts[0][2]):
                    raise Refus('refus Bob non démontré avant expiration')
                children=re.findall(rb'NAEXEC enfant owner='+str(bob[0]).encode()+rb' child=(\d+)',serial.data)
                if a.reactivation:
                    cut=serial.data.index(b'NAR2 Bob nouvelle session explicite distincte')
                    if len(children)!=1 or re.search(rb'NAEXEC enfant owner='+str(bob[0]).encode()+rb' child=',serial.data[:cut]):
                        raise Refus('création Bob avant nouvelle authentification ou doublon')
                elif a.course:
                    beg=re.findall(rb'R2TIME pid='+str(bob[0]).encode()+rb' ticks=(\d+) R2PROBE s=31 i=0 k=0 n=0',serial.data)
                    end=re.findall(rb'R2TIME pid='+str(bob[0]).encode()+rb' ticks=(\d+) R2PROBE s=32 i=0 k=1 n=(\d+)',serial.data)
                    confirm=re.findall(rb'R2TIME pid=\d+ ticks=(\d+) R2PROBE s=20 n=0',serial.data)
                    complete=re.findall(rb'R2TIME pid=\d+ ticks=(\d+) R2PROBE s=22 n=0',serial.data)
                    if len(beg)!=1 or len(end)!=1 or len(confirm)!=1 or len(complete)!=1 or int(end[0][1]) not in (0,10):raise Refus('chronologie course absente')
                    if not (int(beg[0])<=int(complete[0]) and int(end[0][0])>=int(confirm[0])):raise Refus('pas de recouvrement admission/confirmation')
                    terminal=serial.data.index(b'NAR2 epoque RETIREE recu terminal authentifie')
                    if re.search(rb'NAEXEC enfant owner='+str(bob[0]).encode()+rb' child=',serial.data[terminal:]):raise Refus('enfant après RETIREE')
                    if len(children)!=(1 if int(end[0][1])==0 else 0):raise Refus('PID de course absent ou interdit')
                    report['course']={'debut_bob_ticks':int(beg[0]),'fin_bob_ticks':int(end[0][0]),'statut_spawn':int(end[0][1]),'debut_confirmation_ticks':int(confirm[0]),'fin_confirmation_ticks':int(complete[0]),'enfants':[int(v) for v in children],'aucune_creation_apres_RETIREE':True}
                elif children:
                    raise Refus('création Bob supplémentaire ou non identifiée')
                report['ticket_bob']={'reference':int(attempts[0][0]),'presentation_ticks':int(attempts[0][1]),'expiration_ticks':int(attempts[0][2]),'frequence_hz':250,'statut_natif':report['course']['statut_spawn'] if a.course else 10,'enfants_supplementaires':0}
            for pid in set(by_shell) | set(bob):
                if not re.search(rb'NAEXEC restitue pid=' + str(pid).encode() + rb' roots=0 handles=0 waits=0 pages=0 pile=0 threads=0', serial.data):
                    raise Refus('ressources de session non restituées')
            if not re.search(rb'endpoints=0 nodes=0 espaces=0 cadres_ipc=0', serial.data):
                raise Refus('drainage global incomplet')
            level = b'EL0' if a.arch == 'aarch64' else b'Ring3'
            codes = dict((int(p), int(c)) for p, c in re.findall(rb'PROC pid=(\d+) niveau=' + level + rb' pc=0x[0-9a-f]+ sortie=(\d+)', serial.data))
            for item in report['commandes']:
                if item.get('statut_ticket')==8:continue
                if a.dernier_concurrent and len(item['pid'])==2:
                    item['statuts_natifs']=[codes.get(pid) for pid in item['pid']]
                    if item['statuts_natifs']!=[0,0]:raise Refus('ELF concurrents non terminés normalement')
                item['statut_natif'] = codes.get(item['pid'][0])
                if item['statut_natif'] != item['statut_attendu']:
                    raise Refus('statut natif absent ou non conforme')
            if any(codes.get(pid) not in ((0,5,10) if a.course else (0,)) for pid in by_shell.get(bob[0], {})):
                raise Refus('statut natif Bob non conforme')
            observer.close()
            report.update(succes=True, enrolement_epoques=enrolled,
                          fermeture_deux_sessions=True, drainage_global=True)
            return report

        try:
            case = copy.copy(a)
            case.sortie = root / 'invite'
            result = run(case, env, scenario=scenario, rpc_hook=rpc_hook)
            if not a.redemarrage_issue:return result
            if result and not (a.coupure_avant_publication and cut_before_publication):return result
            if a.coupure_avant_publication:
                if result==0 or not cut_before_publication:raise Refus('coupure attendue non exercée')
                with transaction(api.path) as db:
                    if db.execute('SELECT count(*) FROM admin_changes').fetchone()!=(0,) or db.execute('SELECT count(*) FROM mutations').fetchone()!=(0,):raise Refus('mutation inattendue après coupure')
                report['coupure_avant_publication']['resultat_banc_interrompu']=result
            # L'invité A est arrêté et ses ressources drainées avant toute reprise.
            first=copy.deepcopy(report)
            report['succes']=False
            report['reprise']={'resultat':'non terminee'}
            (root/'avant-redemarrage.json').write_text(json.dumps(first,indent=2)+'\n')
            if a.coupure_avant_publication:
                manifest=(root/'manifeste.bin').read_bytes()
                chunks=initial_fragments(root,manifest)
            else:
                paths=list((case.sortie/'captures-invite').glob('*/manifeste.bin'))
                if len(paths)!=1:raise Refus('capture native administrative absente ou ambiguë')
                manifest=paths[0].read_bytes();folder=paths[0].parent
                chunks=[q.read_bytes() for q in sorted(folder.glob('*.chiffre'))]
                if b'NAR2 sauvegarde administrative confirmee' not in (case.sortie/'invite.serial').read_bytes():
                    raise Refus('capture non acquittée dans le registre invité')
            from rpc_recuperation import Dispatcher
            from recuperation import digest
            old_route=route(api)
            packet,sig=api.begin_withdrawal(SERVER,DOMAIN,old_route,signature(admin,'withdraw',old_route))
            (root/'retrait-reprise-en-cours.bin').write_bytes(packet+sig)
            packet,sig=api.drain(SERVER,DOMAIN,old_route)
            (root/'retrait-reprise-definitif.bin').write_bytes(packet+sig)
            keys=[holders[0].keys[0],holders[1].keys[0],holders[0].keys[1],holders[1].keys[1],holders[0].keys[2],holders[1].keys[2]]
            admin.call('mandate',SERVER,DOMAIN,dispatcher.peer_key,dispatcher.measurement,digest(manifest),
                int.from_bytes(manifest[78:86],'big'),manifest[270:302],keys)
            resumed=Dispatcher(dispatcher.authority,dispatcher.enrollment,SERVER,DOMAIN,
                dispatcher.peer_key,dispatcher.measurement,{digest(manifest):manifest},
                {digest(manifest):chunks},
                dispatcher.approve_independently,dispatcher.deliver_independently,dispatcher.retrieve_independently)
            after=copy.copy(case);after.sortie=root/'redemarrage';after.genese=case.sortie/'identites.p9'
            if a.carol_reprise:
                carol_active=True
                for attr,suffix in [('iso','.iso'),('kernel','-kernel.elf'),('registre','-registre.elf')]:
                    setattr(after,attr,a.carol_reprise/(a.arch+suffix))
                if after.registre.read_bytes()!=case.registre.read_bytes():raise Refus('registre différent entre création et authentification Carol')
                a.kernel=after.kernel
            # Même identifiant, aucune seconde confirmation. Conserver la vue
            # attendue du parcours initial, puis le contrôle positif Alice.
            view=cases[-1]
            if a.lot=='groupe':view=('groups --group equipe-r2',group_created)
            cases=[(binary+' --outcome '+intention['operation'],complete),view,('whoami',b'alice\n')]
            a.recu_supprime=False # injection consommée uniquement dans la première VM
            case_status={}
            if a.coupure_avant_publication:
                cases=[(binary+' --outcome '+intention['operation'],b''),('id carol',b''),('whoami',b'alice\n')]
                case_status={0:(20,('Administrative failure: native=22 status=20\nrequest='+intention['operation']+' --outcome\n').encode()),1:(3,b'Administrative failure: native=5 status=3\n')}
                a.coupure_avant_publication=False # injection unique, jamais de CONFIRM en reprise
            if a.membership_r2:
                restarted=True
                a.membership_io=False # aucun ancien handle local n'existe dans cette VM
                cases=[(binary+' --outcome '+intention['operation'],complete),('whoami',b'alice\n')]
            report['commandes']=[];report['evenements']=[];report['succes']=False
            result=run(after,(resumed,holders,admin),scenario=scenario,rpc_hook=rpc_hook)
            report['reprise']={'arret_complet':True,'capture_native':not cut_before_publication,'coupure_avant_admission':cut_before_publication,'ancienne_incarnation':old_route.hex(),
                'nouvelle_incarnation':route(api).hex(),'meme_operation':intention['operation'],'resultat':result}
            if route(api)==old_route:raise Refus('incarnation réutilisée au redémarrage')
            return result
        finally:
            (root / 'commandes-resultats.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    raise SystemExit(main())
