"""Mesures QEMU de phase 3 sur un noyau ipc-smp-lab, avec disques P9 jetables."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

from bloc import Bench, ROOT, command
from fichiers import fixture

CAS = ("libre", "paires", "local", "distant", "calcul", "memoire", "vfs", "sonde", "paires-sonde")
CIBLES = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}


def nombres(ligne):
    return {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)\b", ligne)}


def mesurer(arch, cpu, cas, elf, rep, sortie):
    nom = f"{arch}-{cpu}-{cas}-{rep}"
    serie = sortie / (nom + ".serial")
    env = dict(os.environ, NOVA_ARCH=arch, NOVA_VCPUS=str(cpu), NOVA_KERNEL_ELF=str(elf))
    with tempfile.TemporaryDirectory(dir=sortie) as tmp:
        tmp = Path(tmp)
        iso = tmp / "lab.iso"
        subprocess.run(["sh", ROOT/"scripts/iso.sh", "ipc-smp-"+cas, iso], env=env,
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        argv = ["sh", ROOT/"scripts/qemu.sh"]
        if cas == "vfs":
            disque = Bench(tmp, arch, 4096)
            fixture(disque)
            origine = hashlib.sha256(disque.image.read_bytes()).digest()
            argv = command(arch, 4096, disque.image, readonly=True)
        env["NOVA_ISO"] = str(iso)
        resultats, debut, point = [], None, None
        with serie.open("xb") as trace, (sortie/(nom+".qemu")).open("wb") as erreurs:
            with subprocess.Popen([*argv, "-display", "none", "-monitor", "none", "-serial", "stdio"],
                                  stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=erreurs,
                                  env=env) as vm:
                try:
                    import selectors
                    sel = selectors.DefaultSelector()
                    sel.register(vm.stdout, selectors.EVENT_READ)
                    fragment = b""
                    limite = time.monotonic()+180
                    termine = False
                    while not termine:
                        if not sel.select(max(0, limite-time.monotonic())):
                            raise TimeoutError(nom)
                        bloc = os.read(vm.stdout.fileno(), 65536)
                        if not bloc:
                            raise RuntimeError(f"Arrêt invité dans {serie}")
                        trace.write(bloc)
                        trace.flush()
                        lignes = (fragment+bloc).split(b"\n")
                        fragment = lignes.pop()
                        for ligne in lignes:
                            hote = time.monotonic_ns()
                            s = ligne.decode(errors="replace").strip()
                            if "Panique du noyau" in s or "SMP arret motif=" in s:
                                raise RuntimeError(s)
                            if s.startswith("P3 debut "):
                                debut = hote
                                limite = time.monotonic()+120
                            if s.startswith("P3 fin "):
                                point = dict(**nombres(s), hote_ns=hote-debut, couples=[], hist={}, cpu=[], sites=[])
                                resultats.append(point)
                            if s.startswith("P3 couple "):
                                point["couples"].append(nombres(s)["ns"])
                            if s.startswith("IPC hist "):
                                point["hist"][s.split()[2].split("=")[1]] = nombres(s)
                            if s.startswith("P3 detail "):
                                point["detail"] = nombres(s)
                            if s.startswith("SMP profil "):
                                point["cpu"].append(nombres(s))
                            if s.startswith("SMP section "):
                                point["sites"].append(s.split("site=")[1])
                            if s.startswith("P3 configuration "):
                                config = nombres(s)
                            if s == "P3 validation OK":
                                termine = True
                                break
                    sel.close()
                finally:
                    if vm.poll() is None:
                        vm.kill()
                    vm.wait()
        if cas == "vfs":
            assert hashlib.sha256(disque.image.read_bytes()).digest() == origine
    attendu = cpu if cas in ("paires", "paires-sonde") else 1
    assert (config["cpu"], config["couples"], config["tours"]) == (cpu, attendu, 1000)
    assert [p["serie"] for p in resultats] == list(range(8))
    assert all(len(p["couples"]) == config["couples"] and max(p["couples"]) == p["ns"] for p in resultats)
    if cas in ("sonde", "paires-sonde"):
        assert all(p["hist"]["aller_retour"]["n"] == config["tours"] and len(p["cpu"]) == cpu for p in resultats)
    if cas == "vfs" and "requetes_bloc" in resultats[0]:
        assert all(p["requetes_bloc"] > 0 for p in resultats), "lot sans soumission bloc concurrente"
    return dict(arch=arch, vcpu=cpu, cas=cas, repetition=rep, config=config, releves=resultats,
                charge_fin=os.getloadavg(), elf=hashlib.sha256(elf.read_bytes()).hexdigest())


if __name__ == "__main__":
    arg = argparse.ArgumentParser(description=__doc__)
    arg.add_argument("build", type=Path, help="target-dir des deux noyaux ipc-smp-lab")
    arg.add_argument("sortie", type=Path)
    arg.add_argument("--temoin", type=Path, help="second target-dir à comparer par lancements alternés")
    arg.add_argument("--repetitions", type=int, default=3)
    arg.add_argument("--cas", nargs="+", choices=CAS, default=CAS)
    arg.add_argument("--cpus", nargs="+", type=int, choices=[1,2,4], default=[1,2,4])
    arg.add_argument("--arch", nargs="+", choices=CIBLES, default=list(CIBLES))
    args = arg.parse_args()
    args.sortie.mkdir(parents=True, exist_ok=True)
    for rep in range(args.repetitions):
        cpus = args.cpus[rep % len(args.cpus):] + args.cpus[:rep % len(args.cpus)]
        for cpu in cpus:
            for index, cas in enumerate(args.cas):
                if cpu == 1 and cas == "distant":
                    continue
                for arch in (args.arch if rep % 2 == 0 else reversed(args.arch)):
                    profils = [(args.build, args.sortie)]
                    if args.temoin:
                        profils = [(args.temoin, args.sortie/"avant"), (args.build, args.sortie/"apres")]
                        if (rep + cpus.index(cpu) + index) % 2:
                            profils.reverse()
                    for build, sortie in profils:
                        sortie.mkdir(parents=True, exist_ok=True)
                        elf = build.resolve()/CIBLES[arch]/"release/novaos"
                        dest = sortie/f"{arch}-{cpu}-{cas}-{rep}.json"
                        if dest.exists():
                            assert json.loads(dest.read_text())["elf"] == hashlib.sha256(elf.read_bytes()).hexdigest()
                            continue
                        resultat = mesurer(arch, cpu, cas, elf, rep, sortie)
                        dest.write_text(json.dumps(resultat, indent=2)+"\n")
                        print(dest, flush=True)
