"""Qualification du gestionnaire sur un vrai volume VirtIO P9 jetable par cellule."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import select
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from bloc_qemu import command as block_command
from session_cli import fixture_p9

TARGETS = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}
CONTROL = [
    b"SVC-P3 stockage INFO arret nouvelle-identite anciens-canaux-retires INFO OK",
    b"SVC-P3 READY ancien refuse dependant WAITING sans PID",
    b"SVC-P3 READY courant admis dependant RUNNING",
    b"SVC-P3 dependance obligatoire perdue puis retablie",
    b"SVC-P3 sortie perte de canal et silence detectes",
    b"SVC-P3 delai de disponibilite et boucle de fautes bornes",
    b"SVC-P3 arret force apres delai",
    b"SVC-P3 fermeture volontaire confirmee",
]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def guest(arch, sector, cpus, image, iso, scenario, out, cut):
    env = {**os.environ, "NOVA_ARCH": arch, "NOVA_ISO": str(iso), "NOVA_VCPUS": str(cpus)}
    argv = block_command(arch, sector, image, controls=["-display", "none", "-monitor", "none", "-serial", "stdio"])
    deadline = time.monotonic() + 180
    buf = bytearray()
    # Tous les profils gardent leurs rôles réels. Le rôle lecture ne peut arrêter un service.
    console = scenario in ("cli", "services-admin")
    # Le client observe les quatre incarnations par le protocole public avant de
    # tester INFO. Une console silencieuse reste utilisable pendant leur démarrage.
    commands = [(f"@services status {id}", f"service={id} etat=5 ".encode()) for id in (1, 2, 9, 10)]
    commands.append(("@services info 10", rb"service=10 etat=5 .*valeur=1212367171 "))
    if not cut:
        # cat lance un autre ELF avec un sixième tmp. La coupure ciblée des
        # cinq principaux a lieu avant toute commande qui crée une instance.
        commands.append(("cat /documents/exemple.txt", rb"\r?\nNovaOS\r?\n"))
    commands.append(("@services info 9", rb"service=9 etat=5 .*valeur=1397118542 "))
    if scenario == "services-admin":
        commands.extend([("@services stop 9", rb"service=9 "),
                         ("@services status 9", rb"service=9 etat=7 "),
                         ("@services start 9", rb"service=9 "),
                         ("@services status 9", rb"service=9 etat=5 essai=2 "),
                         ("@services info 9", rb"service=9 etat=5 essai=2 .*valeur=1397118542 retraits=1")])
    else:
        commands.append(("@services stop 9", rb"erreur : [^\r\n]+"))
    command_index = 0
    sent_at = None
    signalled = False
    first_identity = None
    with out.open("xb") as log, subprocess.Popen(argv, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0) as vm:
        try:
            while time.monotonic() < deadline:
                if not select.select([vm.stdout], [], [], 1)[0]:
                    continue
                chunk = os.read(vm.stdout.fileno(), 65536)
                if not chunk:
                    break
                log.write(chunk)
                log.flush()
                buf.extend(chunk)
                if b"Panique du noyau" in buf or b"BLOC session terminee ok=false" in buf:
                    # La console transmet la panique octet par octet. Conserver le diagnostic.
                    until = time.monotonic() + 1
                    while time.monotonic() < until:
                        if select.select([vm.stdout], [], [], 0.1)[0]:
                            rest = os.read(vm.stdout.fileno(), 65536)
                            if not rest:
                                break
                            log.write(rest)
                            buf.extend(rest)
                    break
                if console and b"nsh> " in buf:
                    assert b"SVC stockage INFO disponible" not in buf
                    assert b"SVC tmp prive ordinaire" not in buf
                    if not signalled and sent_at is None:
                        command, _ = commands[command_index]
                        sent_at = len(buf)
                        vm.stdin.write(command.encode() + b"\n")
                    if sent_at is not None:
                        segment = buf[sent_at:]
                        first_line = segment.find(b"\r\n")
                        if first_line >= 0 and b"nsh> " in segment[first_line+2:]:
                            command, expected = commands[command_index]
                            match = re.search(expected, segment)
                            if not match and command.startswith("@services status "):
                                # L'arrêt et le démarrage sont asynchrones. Interroger l'état réel.
                                sent_at = len(buf)
                                vm.stdin.write(command.encode()+b"\n")
                            else:
                                assert match, (command, segment[-1000:])
                                if command == "@services info 9":
                                    identity, = re.findall(rb"instance=([a-f0-9/]+)", segment)
                                    if first_identity is None:
                                        first_identity = identity
                                    else:
                                        assert identity != first_identity
                                    if cut:
                                        vm.kill()
                                        assert vm.wait(timeout=10) == -signal.SIGKILL
                                        return bytes(buf)
                                command_index += 1
                                sent_at = None
                                if command_index == len(commands):
                                    signalled = True
                                    vm.stdin.write(b"exit\n")
                                else:
                                    # L'invite est déjà reçue ; une VM au repos peut rester silencieuse.
                                    sent_at = len(buf)
                                    vm.stdin.write(commands[command_index][0].encode()+b"\n")
                if b"BLOC session terminee ok=true" in buf and re.search(rb"SMP arret motif=normal[^\r\n]*\r?\n", buf):
                    mask = hex((1 << cpus) - 1)
                    assert f"attendus={mask} en_ligne={mask} ack={mask} absents=0x0 muets=0x0".encode() in buf
                    if console:
                        assert command_index == len(commands) and signalled
                    elif scenario == "services-capacites-race":
                        assert b"SVC-CAP CAS 12 reponses OK BUSY restart concurrence" in buf
                        assert b"SVC-CAP restauration UNCERTAIN generation 14 relue deux fois, handle revoque NVFS OK" in buf
                    elif scenario == "services-config-race":
                        markers = (b"deux publications CAS 0 en vol : une confirmee, une BUSY, generation 1 relue",
                                   b"resultat client UNCERTAIN apres admission et revocation",
                                   b"deux relectures P9 generation 2, handle retire, aucune repetition")
                        assert all(m in buf for m in markers), [m for m in markers if m not in buf]
                    elif scenario == "services-test":
                        assert all(s in buf for s in CONTROL), [s for s in CONTROL if s not in buf]
                    else:
                        assert b"SVC-P3 perte gestionnaire canal revoque" in buf
                    return bytes(buf)
        finally:
            if vm.poll() is None:
                vm.kill()
            vm.wait(timeout=10)
    raise RuntimeError(f"{out} : {bytes(buf[-1600:]).decode(errors='replace')}")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--arch", required=True, choices=TARGETS)
    ap.add_argument("--vcpus", required=True, type=int, choices=(1, 2, 4))
    ap.add_argument("--secteur", required=True, type=int, choices=(512, 4096))
    ap.add_argument("--preuves", required=True, type=Path)
    ap.add_argument("--oracle", required=True, type=Path)
    opts = ap.parse_args()
    out = opts.preuves.resolve()
    out.mkdir(parents=True, exist_ok=False)
    image = out / "volume.raw"
    fixture_p9(image, opts.secteur)
    oracle = [opts.oracle.resolve(), image, str(opts.secteur)]
    catalogue = subprocess.check_output([*oracle, "catalogue"])
    kernel = Path(os.environ.get("NOVA_KERNEL_ELF", ROOT / "target" / TARGETS[opts.arch] / "release/novaos"))
    # Figer le binaire avant la première ISO. Une construction d'un autre profil
    # ne peut ensuite changer le noyau ni l’empreinte attribuée aux quatre essais.
    frozen = out / "novaos.elf"
    frozen.write_bytes(kernel.read_bytes())
    digest = sha(frozen.read_bytes())
    env = {**os.environ, "NOVA_ARCH": opts.arch, "NOVA_KERNEL_ELF": str(frozen)}
    qemu = subprocess.check_output([f"qemu-system-{opts.arch}", "--version"], text=True).splitlines()[0]
    rows = []
    scenarios = [("cli", True), ("services-test", False), ("services-perte", False), ("cli", False)]
    if opts.vcpus == 2:
        scenarios.append(("services-admin", False))
    for index, (scenario, cut) in enumerate(scenarios):
        iso = out / (scenario + ".iso")
        if not iso.exists():
            subprocess.run(["sh", ROOT / "scripts/iso.sh", scenario, iso], env=env, check=True,
                           stdout=subprocess.DEVNULL)
        label = f"{index}-{scenario}"
        raw = guest(opts.arch, opts.secteur, opts.vcpus, image, iso, scenario, out/(label+".serial"), cut)
        disk = image.read_bytes()
        snapshot = out/(label+".raw.gz")
        with gzip.open(snapshot, "wb") as dest:
            dest.write(disk)
        with gzip.open(snapshot, "rb") as saved:
            assert saved.read() == disk
        state = "services-actifs" if cut else "propre"
        checked = subprocess.check_output([*oracle, state], text=True)
        # Le premier démarrage ordinaire publie le journal dans un objet privé.
        # Valider son format P9 puis retirer exactement sa ligne de la comparaison.
        journal = subprocess.check_output([*oracle, "objet-journal"])
        entrees = subprocess.check_output([*oracle, "catalogue"]).splitlines(keepends=True)
        assert entrees.count(journal) == 1
        entrees.remove(journal)
        assert catalogue == b"".join(entrees)
        assert disk == image.read_bytes(), "écriture par la relecture indépendante"
        memory = re.search(rb"cadres_init=(\d+) cadres_fin=(\d+) allocations=(\d+) pic_cadres=(\d+) buddy_init=(\d+) buddy_fin=(\d+)", raw)
        row = dict(profil=scenario, coupure_qemu=cut, qemu=qemu, architecture=opts.arch, vcpus=opts.vcpus,
                   secteur=opts.secteur, noyau_sha256=digest, iso_sha256=sha(iso.read_bytes()),
                   journal_sha256=sha(raw), image_sha256=sha(disk), catalogue_sha256=sha(catalogue), oracle=checked)
        if scenario in ("services-test", "services-perte"):
            assert memory, "mesure de collecte manquante"
            def accounting(phase):
                line = next(line for line in raw.splitlines() if b"RAM phase=" + phase + b" " in line)
                return {k.decode(): int(v) for k, v in re.findall(rb"(\w+)=(\d+)", line)}
            initial, final = accounting(b"bloc_repos"), accounting(b"bloc_collecte")
            for key in ("processus", "ipc", "utilisateur", "upile", "kpile", "heap", "buddy", "meta_actifs"):
                assert final[key] == initial[key], (key, initial, final)
            assert initial["libres"] - final["libres"] == sum(final[k] - initial[k] for k in ("dma", "tables"))
            assert memory[5] == memory[6]
            row["cadres_plateforme"] = {k: final[k]-initial[k] for k in ("dma", "tables")}
        if memory:
            row["memoire"] = dict(zip(("cadres_avant","cadres_apres","allocations","pic_cadres",
                                      "buddy_avant","buddy_apres"), map(int, memory.groups())))
        rows.append(row)
        print(opts.arch, opts.vcpus, opts.secteur, label, checked.strip(), flush=True)
    (out/"resultats.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
