"""Comparer trois exécutables sur les six lots Darwin, une série par invocation."""
import argparse
import fcntl
import hashlib
import itertools
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "livraison-realpath-0240/lab"))
from options import GNU, NOVA, observe
from performances import measure

PUBLISHED = "e2637f692f7ad65313258d2396df21852b0c8d99"
LOTS = [
    ("normalisé", "-e", "a"),
    ("profond", "-e", "a/b/c/d/e/f/g/h"),
    ("lien physique", "-P", "lien/.."),
    ("lien logique", "-L", "lien/.."),
    ("lexical", "-ms", "a/b/c/d/e/f/g/h/../../i"),
    ("erreurs", "-e", "absent/x"),
]
COUNT = 512


def digest(data):
    return hashlib.sha256(data).hexdigest()


def host_load():
    # N'enregistrer que des mesures agrégées de l'hôte partagé.
    processes = subprocess.check_output(["ps", "-A", "-o", "pcpu=", "-o", "comm="], text=True)
    cpu = 0.0
    emulators = 0
    for row in processes.splitlines():
        pct, command = row.strip().split(None, 1)
        cpu += float(pct)
        emulators += Path(command).name.startswith("qemu-system-")
    return dict(charge=os.getloadavg(), pct_cpu_total=cpu, qemu=emulators)


def summary(samples):
    times = [s["ns"] for s in samples]
    q1, _, q3 = statistics.quantiles(times, n=4, method="inclusive")
    median = statistics.median(times)
    return dict(mediane_ns=median, q1_ns=q1, q3_ns=q3,
                mad_ns=statistics.median(abs(t - median) for t in times),
                min_ns=min(times), max_ns=max(times),
                rss_max_octets=max(s["rss"] for s in samples))


def run(programs, rng, report):
    loads = LOTS.copy()
    rng.shuffle(loads)
    deep = Path("a/b/c/d/e/f/g/h")
    deep.mkdir(parents=True)
    Path("lien").symlink_to(deep)
    with open(os.devnull, "wb") as devnull:
        for label, flag, path in loads:
            argv = [flag, *([path] * COUNT)]
            oracle = observe(GNU, argv, {"LC_ALL": "C"})
            samples = {key: [] for key in programs}
            for exe in programs.values():
                assert observe(exe, argv, {"LC_ALL": "C"}) == oracle
            orders = list(itertools.permutations(programs))
            warmup = list(programs)
            rng.shuffle(warmup)
            for key in warmup:
                measure(programs[key], argv, devnull.fileno(), oracle["statut"])
            cells = []
            before = host_load()
            for block in range(3):
                rng.shuffle(orders)
                for order in orders:
                    for key in order:
                        result = measure(programs[key], argv, devnull.fileno(), oracle["statut"])
                        cells.append(dict(bloc=block, programme=key, **result))
                        samples[key].append(result)
            after = host_load()
            for exe in programs.values():
                assert observe(exe, argv, {"LC_ALL": "C"}) == oracle
            stats = {key: summary(rows) for key, rows in samples.items()}
            # Les médianes de 18 entiers valent toujours un nombre entier de demi-ns.
            # Comparaison exacte avec 11/10, distincte de l'arrondi d'affichage.
            twice = {k: int(2 * v["mediane_ns"]) for k, v in stats.items()}
            report["lots"].append(dict(charge=label, mode=flag, chemin=path, nombre=COUNT,
                oracle=oracle, avant=before, apres=after, passages=cells, statistiques=stats,
                ratio=twice["candidat"]/twice["gnu"], conforme=10*twice["candidat"] <= 11*twice["gnu"]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidat", type=Path)
    parser.add_argument("sortie", type=Path)
    parser.add_argument("--serie", choices=[1, 2], type=int, required=True)
    args = parser.parse_args()
    programs = dict(gnu=GNU, publie=NOVA, candidat=args.candidat.resolve(strict=True))
    data = subprocess.check_output(["git", "show", PUBLISHED + ":livraison-realpath-0240/bin/realpath"], cwd=ROOT)
    assert NOVA.read_bytes() == data
    version = subprocess.check_output([GNU, "--version"], text=True).splitlines()[0]
    assert version.endswith(" 9.12")
    report = dict(serie=args.serie, plateforme=platform.platform(), machine=platform.machine(),
                  cpu=subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip(),
                  sha_publie=PUBLISHED, version_gnu=version, locale="C", lots=[],
                  sha256={k:digest(p.read_bytes()) for k,p in programs.items()})
    out = args.sortie.resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    if out.exists():
        parser.error("le fichier de résultats existe déjà")
    previous = Path.cwd()
    with (Path(os.environ["NOVAOS_TMPDIR"])/"arc027-mesures.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as tmp:
            try:
                os.chdir(tmp)
                run(programs, random.Random(27000 + args.serie), report)
            finally:
                os.chdir(previous)
    worst = max(report["lots"], key=lambda row: row["ratio"])
    report.update(pire_charge=worst["charge"], pire_ratio=worst["ratio"])
    with out.open("x") as dest:
        json.dump(report, dest, indent=1, ensure_ascii=False)
        dest.write("\n")
    for row in report["lots"]:
        print(row["charge"], row["ratio"], "OK" if row["conforme"] else "ÉCHEC")
    if not all(row["conforme"] for row in report["lots"]):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
