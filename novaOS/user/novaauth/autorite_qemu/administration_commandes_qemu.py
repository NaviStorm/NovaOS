"""Six ELF administratifs réels depuis nsh ; consultations et versions seulement.

Le lanceur externe détient le verrou commun QEMU. Bob conserve son parcours de
refus historique. Alice commence ensuite une nouvelle authentification initiale,
sans renouvellement implicite. Ce banc ne qualifie aucune mutation administrative.
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('arch', choices=['aarch64', 'x86_64'])
    parser.add_argument('sector', type=int, choices=[512, 4096])
    for name in ('iso', 'registre', 'genese', 'cles', 'decision', 'secrets',
                 'sortie', 'detenteur', 'capture', 'triplet', 'kernel'):
        parser.add_argument('--' + name, type=lambda v: Path(v).resolve(), required=True)
    a = parser.parse_args()
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
              'portee': 'trois consultations et versions des trois ELF de mutation ; aucune mutation',
              'commandes': []}
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

        def rpc_hook(phase, operation, body, result, qmp, disk):
            nonlocal enrolled
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
            serial.expect(rb'NAEXEC registre pret puis connexion du vrai nsh', 100)
            # Ce parcours indépendant des mutations commence le bail d'Alice
            # après Bob, au lieu de consommer ses 60 secondes pendant l'attente.
            serial.expect(rb'NAR2 Bob fermeture acquittee', 100)
            if b'NAR2 Bob refus serveur et substitutions verifies' not in serial.data:
                raise Refus('témoin négatif Bob absent')
            for i, (command, expected) in enumerate(cases):
                serial.offset = len(serial.data)
                start = serial.offset
                serial.submit(command + '; echo NXR_$?__FIN' + ('; exit' if i == len(cases)-1 else ''))
                status = serial.expect(rb'NXR_(\d+)__FIN\r?\n', 80)
                if i != len(cases)-1:
                    serial.expect(rb'nsh> ', 20)
                output = bytes(serial.data[start:serial.offset])
                (out / f'commande-{i:02}.bin').write_bytes(output)
                pids = set(int(p) for p in re.findall(rb'NXFLUX pid=(\d+) fd=', output))
                item = {'commande': command, 'statut_shell': int(status[1]), 'pid': sorted(pids)}
                report['commandes'].append(item)
                if int(status[1]) != 0 or len(pids) != 1:
                    raise Refus('statut ou PID non conforme : ' + command)
                pid = next(iter(pids))
                rows = [(binding, closed, data) for binding, closed, data in observer.outputs()
                        if binding.child == pid]
                if len(rows) != 2 or any(not closed for _, closed, _ in rows):
                    raise Refus('sorties non drainées : ' + command)
                streams = {b.fd: data for b, _, data in rows}
                for fd, data in streams.items():
                    (out / f'commande-{i:02}-fd{fd}.bin').write_bytes(data)
                if streams != {1: expected, 2: b''}:
                    raise Refus('octets non conformes : ' + command)
                item['stdout_octets'] = len(expected)
                item['stderr_octets'] = 0
                item['octets_conformes'] = True
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
            bob = [s for s, children in by_shell.items() if list(children.values()) == [{1: bob_expected, 2: b''}]]
            if len(by_shell) != 2 or len(bob) != 1:
                raise Refus('sorties Bob confondues ou absentes')
            for pid in by_shell:
                if not re.search(rb'NAEXEC restitue pid=' + str(pid).encode() + rb' roots=0 handles=0 waits=0 pages=0 pile=0 threads=0', serial.data):
                    raise Refus('ressources de session non restituées')
            if not re.search(rb'endpoints=0 nodes=0 espaces=0 cadres_ipc=0', serial.data):
                raise Refus('drainage global incomplet')
            level = b'EL0' if a.arch == 'aarch64' else b'Ring3'
            codes = dict((int(p), int(c)) for p, c in re.findall(rb'PROC pid=(\d+) niveau=' + level + rb' pc=0x[0-9a-f]+ sortie=(\d+)', serial.data))
            for item in report['commandes']:
                item['statut_natif'] = codes.get(item['pid'][0])
                if item['statut_natif'] != 0:
                    raise Refus('statut natif absent ou non nul')
            if any(codes.get(pid) != 0 for pid in by_shell[bob[0]]):
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
