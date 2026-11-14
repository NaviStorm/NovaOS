"""Qualification fonctionnelle d'équité IPC en VM. Requiert un ELF ipc-equite-lab."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

from ipc_multicoeur import CIBLES, ROOT


def champs(ligne):
    return {k: v if k == "role" else int(v, 0)
            for k, v in re.findall(r"(\w+)=(\w+)", ligne)}


def verifier(trace, cpu):
    lignes = trace.splitlines()
    debut, = [champs(l) for l in lignes if l.startswith("EQUITE debut ")]
    assert debut["cpu"] == cpu and debut["couples"] == 4
    periodes = [champs(l) for l in lignes if l.startswith("EQUITE periode=")]
    assert [p["periode"] for p in periodes] == list(range(debut["periodes"]))
    assert all(p["vivants"] == 10 for p in periodes) and periodes[-1]["preemptions"] > 0
    fils = [champs(l) for l in lignes if l.startswith("EQUITE fil=")]
    assert sorted(f["role"] for f in fils) == sorted(
        ["client", "serveur"] * 4 + ["calcul", "file_pleine", "proprietaire", "revoque", "revoque"])
    assert len({f["generation"] for f in fils}) == len(fils)
    for f in fils:
        assert f["departs"] > 0 and 0 < f["cpus"] < (1 << cpu)
        assert f["progres"] >= f["minimum"] > 0
        if f["role"] == "calcul":
            assert f["preemptions"] > 0
        if f["role"] == "revoque":
            assert f["reveils"] >= 1 and f["bloque_ns"] > 0
    if cpu > 1:
        assert all((f["cpus"] & 3) == 3 for f in fils[:2])
    intervalles = [champs(l) for l in lignes if l.startswith("EQUITE intervalle=")]
    soutenus = [f for f in fils if f["role"] in ("client", "serveur", "calcul", "file_pleine")]
    assert len(intervalles) == len(periodes) * len(soutenus)
    for f in soutenus:
        rangs = [d for d in intervalles if d["generation"] == f["generation"]]
        assert [d["intervalle"] for d in rangs] == list(range(len(periodes)))
        assert min(d["delta"] for d in rangs) == f["minimum"]
        assert sum(d["delta"] for d in rangs) <= f["progres"]
    couples = [champs(l) for l in lignes if l.startswith("EQUITE couple=")]
    assert [c["couple"] for c in couples] == list(range(debut["couples"]))
    assert all(c["tours"] > 0 for c in couples)
    fin, = [champs(l) for l in lignes if l.startswith("EQUITE validation OK ")]
    assert fin["pages_ipc"] == 0 and fin["cadres"] > 0
    return dict(debut=debut, periodes=periodes, intervalles=intervalles, fils=fils, couples=couples, bilan=fin)


def executer(arch, cpu, elf, dest, delai):
    env = dict(os.environ, NOVA_ARCH=arch, NOVA_VCPUS=str(cpu), NOVA_KERNEL_ELF=str(elf))
    nom = f"{arch}-{cpu}"
    trace = dest / (nom + ".serial")
    with tempfile.TemporaryDirectory(dir=dest) as temp:
        iso = Path(temp) / "equite.iso"
        subprocess.run(["sh", ROOT / "scripts/iso.sh", "ipc-smp-equite", iso], check=True,
                       env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        env["NOVA_ISO"] = str(iso)
        argv = ["sh", ROOT / "scripts/qemu.sh", "-display", "none", "-monitor", "none",
                "-serial", "stdio"]
        start = time.monotonic()
        with trace.open("wb") as out, (dest / (nom + ".qemu")).open("wb") as err:
            with subprocess.Popen(argv, env=env, stdout=out, stderr=err, stdin=subprocess.DEVNULL) as vm:
                try:
                    while time.monotonic() - start < delai:
                        journal = trace.read_text(errors="replace")
                        if "SMP arret motif=normal" in journal:
                            break
                        if "Panique du noyau" in journal or "SMP arret motif=" in journal or vm.poll() is not None:
                            raise RuntimeError(f"{nom}\n{journal[-1600:]}")
                        time.sleep(.1)
                    else:
                        raise TimeoutError(f"{nom} — {delai}s\n{journal[-1600:]}")
                finally:
                    if vm.poll() is None:
                        vm.kill()
                    vm.wait()
    r = verifier(journal, cpu)
    r.update(architecture=arch, cpu=cpu, elf_sha256=hashlib.sha256(elf.read_bytes()).hexdigest(),
             duree_hote_s=round(time.monotonic()-start, 3), delai_hote_s=delai)
    (dest / (nom + ".json")).write_text(json.dumps(r, indent=2) + "\n")
    print(f"OK — {nom}, {len(r['fils'])} acteurs", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build", type=Path, help="target-dir de la construction ipc-equite-lab")
    parser.add_argument("sortie", type=Path)
    parser.add_argument("--arch", nargs="+", choices=CIBLES, default=list(CIBLES))
    parser.add_argument("--cpus", nargs="+", type=int, choices=(1, 2, 4), default=[1, 2, 4])
    parser.add_argument("--delai", type=int, default=120, help="durée maximale par VM, en secondes hôte")
    args = parser.parse_args()
    args.sortie.mkdir(parents=True, exist_ok=True)
    for arch in args.arch:
        for cpu in args.cpus:
            executer(arch, cpu, args.build.resolve()/CIBLES[arch]/"release/novaos", args.sortie, args.delai)
