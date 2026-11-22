"""ELF administratifs réels depuis nsh ; parcours indépendants et bornés.

Le lanceur externe détient le verrou commun QEMU. Bob conserve son parcours de
refus historique. Alice commence ensuite une nouvelle authentification initiale,
sans renouvellement implicite. Le lot groupe exerce préparation, confirmation
explicite et consultation du même reçu, sans répétition de mutation.
"""
import argparse
import copy
import json
import re
from contextlib import ExitStack
from pathlib import Path

from recuperation_qemu import fixture, run, SERVER, DOMAIN
from exec_deux_vm import route, signature
from reprise import Refus, transaction
import admission_exec as execs
from administration_epochs import Epochs, initialize
from administration_events import Events, command_status_pattern, observed_command_status_pattern


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('arch', choices=['aarch64', 'x86_64'])
    parser.add_argument('sector', type=int, choices=[512, 4096])
    parser.add_argument('--lot', choices=['lectures', 'versions', 'groupe', 'dernier-administrateur', 'desactivation'], required=True)
    parser.add_argument('--commande', choices=['whoami', 'id', 'groups', 'useradd', 'usermod', 'groupadd'])
    parser.add_argument('--reactivation',action='store_true',help='recette S9 après retrait effectif')
    parser.add_argument('--securite',action='store_true',help='sonde directe étendue, distincte du témoin temporel')
    parser.add_argument('--ticket-vivant',action='store_true',help='désactivation pendant que Bob conserve un ticket non consommé')
    parser.add_argument('--reauth-issue', action='store_true',
                        help='authentification explicite neuve après publication, avant consultation de son issue')
    parser.add_argument('--recu-supprime', action='store_true',
                        help='supprimer une fois le reçu après publication durable ; conserver UNCERTAIN')
    for name in ('iso', 'registre', 'genese', 'cles', 'decision', 'secrets',
                 'sortie', 'detenteur', 'capture', 'triplet', 'kernel'):
        parser.add_argument('--' + name, type=lambda v: Path(v).resolve(), required=True)
    a = parser.parse_args()
    a.ticket_vivant = a.ticket_vivant or a.reactivation
    if a.securite and a.ticket_vivant:parser.error('les deux profils sont distincts')
    if a.ticket_vivant and a.lot!='desactivation':
        parser.error('le témoin de ticket vivant exige le lot désactivation')
    if (a.reauth_issue or a.recu_supprime) and a.lot not in ('groupe','desactivation'):
        parser.error('options de publication réservées aux lots de mutation')
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
              'commandes': [], 'drapeaux_locale': 0, 'langue_ELF': 'en'}
    raw = a.triplet.read_bytes()
    size = int.from_bytes(raw[152:156], 'big')
    users = json.loads(raw[156:156 + size])
    subjects = tuple((bytes(u['id']), u['authentication']['epoch'])
                     for u in users if u['status'] == 'Active')
    ids = a.genese.with_name('identites-temoins.bin').read_bytes()
    if len(ids) != 48:
        raise Refus('identités témoins invalides')
    alice, _, group = (ids[i:i+16].hex() for i in (0, 16, 32))
    cases = [('whoami', b'alice\n'),
             ('id', f'subject={alice} name=alice state=ACTIVE groups=base({group})\n'.encode()),
             ('groups', b'base\n')]
    cases += [(n + ' --version', (n + ' 0.24.0 NovaOS NXADMIN1 experimental\n').encode())
              for n in ('useradd', 'usermod', 'groupadd')]
    # Deux parcours indépendants : le total de connexions du relais hôte reste
    # borné à 128 et chaque parcours commence avec une nouvelle session.
    # L'essai initial de six commandes ayant saturé ce relais est conservé.
    cases = cases[:3] if a.lot == 'lectures' else cases[3:]
    if a.lot in ('groupe','desactivation'):
        intention = {}

        def preparation(data):
            match = re.fullmatch(rb'request=([0-9a-f]{32}) confirmation=([0-9a-f]{64}) state=PREPARED\n', data)
            if not match:
                raise Refus('préparation non canonique')
            intention.update(operation=match[1].decode(), confirmation=match[2].decode())
            report['intention'] = dict(intention)
            return data

        def complete(_):
            return ('request=' + intention['operation'] + ' generation=24 state=COMPLETE\n').encode()

        command='groupadd equipe-r2' if a.lot=='groupe' else 'usermod --disable bob'
        binary=command.split()[0]
        report['portee']='création de groupe' if a.lot=='groupe' else 'désactivation de Bob après fermeture de sa session ; aucun témoin de ticket vivant'
        if a.ticket_vivant:report['portee']='désactivation de Bob avec ticket vivant ; refus avant expiration exigé'
        cases = [(command+' --prepare', preparation),
                 (lambda: command+' --confirm ' + intention['confirmation'] +
                  ' --request ' + intention['operation'], complete),
                 (lambda: binary+' --outcome ' + intention['operation'], complete)]
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
            cases += [('usermod --enable bob --prepare',prepare_reactivation),
                      (lambda:'usermod --enable bob --confirm '+second['confirmation']+' --request '+second['operation'],reactivated),
                      (lambda:'usermod --outcome '+second['operation'],reactivated)]
    elif a.lot == 'dernier-administrateur':
        cases = [('usermod --disable alice --prepare', b'')]
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

        def rpc_hook(phase, operation, body, result, qmp, disk):
            nonlocal enrolled, discarded_receipt
            if phase == 'after' and operation == 50 and a.recu_supprime and not discarded_receipt:
                discarded_receipt = True
                # L'autorité a déjà décidé. Le banc supprime uniquement le reçu,
                # jamais la mutation, et ne soumet aucune seconde demande.
                report['recu_supprime_apres_publication'] = True
                raise Refus('UNCERTAIN : reçu administratif supprimé par le banc')
            if phase == 'before' and operation == 40 and not enrolled:
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
            if not a.ticket_vivant:
                serial.expect(rb'NAR2 Bob fermeture acquittee', 100)
                if b'NAR2 Bob refus serveur et substitutions verifies' not in serial.data:
                    raise Refus('témoin négatif Bob absent')
            for i, (command, expected) in enumerate(cases):
                expected_status = 3 if a.lot == 'dernier-administrateur' else 0
                expected_stderr = b''
                if a.lot == 'dernier-administrateur':
                    expected_stderr = b'Administrative failure: native=5 status=3\n'
                elif a.recu_supprime and i == 1:
                    expected_status = 20
                    expected = b''
                    expected_stderr = ('Administrative failure: native=22 status=20\nrequest=' +
                                       intention['operation'] + ' --outcome\n').encode()
                if a.reauth_issue and i == 2:
                    observer.expect_successor(report['commandes'][0]['shell'])
                    serial.submit('session reauthentifier')
                    serial.expect('NovaAuth : nouvelle session authentifiée'.encode(), 40)
                    serial.expect(rb'nsh> ', 20)
                    report['nouvelle_authentification_explicite_avant_issue'] = True
                if a.reactivation and i==4:
                    observer.expect_successor(events.actor(b'NAR2 Bob attente declenchement authentification'))
                if callable(command):
                    command = command()
                # Identité d'étape distincte ; aucune réponse d'une commande
                # précédente ne peut terminer la commande courante.
                step = 'NXR2_' + str(i) + '_'
                line=command + '; echo ' + step + '$?__FIN' + ('; exit' if i == len(cases)-1 else '')
                serial.offset = len(serial.data)
                start = serial.offset
                serial.submit(line)
                pattern=observed_command_status_pattern(step) if a.ticket_vivant else command_status_pattern(step)
                status = serial.expect(pattern, 80)
                shell_status=int(status[2] if a.ticket_vivant else status[1])
                if i != len(cases)-1:
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
                if shell_status != expected_status or len(pids) != 1:
                    raise Refus('statut ou PID non conforme : ' + command)
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
                if a.ticket_vivant and int(status[1]) != item['shell']:
                    raise Refus('statut provenant d’une autre instance')
                for fd, data in streams.items():
                    (out / f'commande-{i:02}-fd{fd}.bin').write_bytes(data)
                if callable(expected):
                    expected = expected(streams.get(1, b''))
                if streams != {1: expected, 2: expected_stderr}:
                    raise Refus('octets non conformes : ' + command)
                item['stdout_octets'] = len(expected)
                item['stderr_octets'] = len(expected_stderr)
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

                if a.reactivation and i==4:
                    bob_pid=events.actor(b'NAR2 Bob attente declenchement authentification')
                    for message in [b'NAR2 Bob nouvelle session explicite distincte',
                                    b'NAR2 Bob apres reactivation ancien ticket=10 ancienne session=5',
                                    b'NAR2 Bob nouveau lancement autorise droits ordinaires',
                                    b'NAR2 Bob fermeture acquittee']:
                        events.expect(bob_pid,message,40)
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
                elif bob[0] in by_shell or len(by_shell)!=1:
                    raise Refus('sortie enfant Bob inattendue dans le témoin temporel')
            elif len(by_shell) != 2 or len(bob) != 1:
                raise Refus('sorties Bob confondues ou absentes')
            if a.ticket_vivant:
                if b'NAR2 Bob ancien ticket statut=10 aucun enfant' not in serial.data:
                    raise Refus('ancien ticket Bob sans refus REVOKED')
                attempts=re.findall(rb'NAEXEC attente owner='+str(bob[0]).encode()+rb' ticket=(\d+) maintenant=(\d+) echeance=(\d+)',serial.data)
                if not attempts or (not a.reactivation and len(attempts)!=1) or int(attempts[0][1])>=int(attempts[0][2]):
                    raise Refus('refus Bob non démontré avant expiration')
                children=re.findall(rb'NAEXEC enfant owner='+str(bob[0]).encode()+rb' child=(\d+)',serial.data)
                if a.reactivation:
                    cut=serial.data.index(b'NAR2 Bob nouvelle session explicite distincte')
                    if len(children)!=1 or re.search(rb'NAEXEC enfant owner='+str(bob[0]).encode()+rb' child=',serial.data[:cut]):
                        raise Refus('création Bob avant nouvelle authentification ou doublon')
                elif children:
                    raise Refus('création Bob supplémentaire ou non identifiée')
                report['ticket_bob']={'reference':int(attempts[0][0]),'presentation_ticks':int(attempts[0][1]),'expiration_ticks':int(attempts[0][2]),'frequence_hz':250,'statut_natif':10,'enfants_supplementaires':0}
            for pid in set(by_shell) | set(bob):
                if not re.search(rb'NAEXEC restitue pid=' + str(pid).encode() + rb' roots=0 handles=0 waits=0 pages=0 pile=0 threads=0', serial.data):
                    raise Refus('ressources de session non restituées')
            if not re.search(rb'endpoints=0 nodes=0 espaces=0 cadres_ipc=0', serial.data):
                raise Refus('drainage global incomplet')
            level = b'EL0' if a.arch == 'aarch64' else b'Ring3'
            codes = dict((int(p), int(c)) for p, c in re.findall(rb'PROC pid=(\d+) niveau=' + level + rb' pc=0x[0-9a-f]+ sortie=(\d+)', serial.data))
            for item in report['commandes']:
                item['statut_natif'] = codes.get(item['pid'][0])
                if item['statut_natif'] != item['statut_attendu']:
                    raise Refus('statut natif absent ou non conforme')
            if any(codes.get(pid) != 0 for pid in by_shell.get(bob[0], {})):
                raise Refus('statut natif Bob non conforme')
            observer.close()
            report.update(succes=True, enrolement_epoques=enrolled,
                          fermeture_deux_sessions=True, drainage_global=True)
            return report

        try:
            case = copy.copy(a)
            case.sortie = root / 'invite'
            return run(case, env, scenario=scenario, rpc_hook=rpc_hook)
        finally:
            (root / 'commandes-resultats.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    raise SystemExit(main())
