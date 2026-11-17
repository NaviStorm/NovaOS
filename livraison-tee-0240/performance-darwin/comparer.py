"""Comparaison Darwin sur fichiers ordinaires. Aucun passage mesuré n’est retiré."""
import argparse
import collections
import datetime
import hashlib
import itertools
import json
import os
import platform
import random
import statistics
import subprocess
import tempfile
import time
from fractions import Fraction
from pathlib import Path

MIO = 1024**2
CASES = list(itertools.product((1, 8, 32), (1, 4)))
GNU = Path("/opt/homebrew/bin/gtee")


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def load_snapshot():
    # Ne conserver ni arguments de processus ni identifiants utilisateur.
    names = subprocess.check_output(["ps", "-Aco", "comm="], text=True).splitlines()
    active = collections.Counter(Path(n.strip()).name for n in names if any(
        k in n.lower() for k in ("qemu-system", "rustc", "cargo", "python", "synology")))
    return {"heure": datetime.datetime.now().astimezone().isoformat(),
            "charge": os.getloadavg(), "processus": active}


def sample(exe, source, dests, expected):
    for dest in dests:
        dest.write_bytes(b"")
    with source.open("rb") as src, dests[0].open("wb") as dst, tempfile.TemporaryFile(dir=source.parent) as err:
        start = time.monotonic_ns()
        proc = subprocess.Popen([exe, *dests[1:]], stdin=src, stdout=dst, stderr=err)
        _, status, usage = os.wait4(proc.pid, 0)
        ns = time.monotonic_ns() - start
        proc.returncode = os.waitstatus_to_exitcode(status)
        err.seek(0)
        diagnostic = err.read()
        if proc.returncode or diagnostic:
            raise RuntimeError((exe, proc.returncode, diagnostic))
    hashes = [sha(d) for d in dests]
    size = source.stat().st_size
    if any(h != expected or d.stat().st_size != size for d,h in zip(dests, hashes)):
        raise ValueError("Sortie différente du corpus")
    return dict(ns=ns, user_s=usage.ru_utime, system_s=usage.ru_stime,
                rss_octets=usage.ru_maxrss, involontaires=usage.ru_nivcsw,
                sha256_sorties=hashes, debit_total_Mio_s=size*len(dests)*1e9/(MIO*ns))


def summary(rows, binaries):
    result = []
    for size, count in CASES:
        line = dict(mio=size, sorties=count, outils={})
        medians = {}
        for label in binaries:
            data = [r for r in rows if r["mio"]==size and r["sorties"]==count
                    and r["outil"]==label and r["tour"]>=0]
            times = sorted(r["ns"] for r in data)
            n = len(times)
            median = Fraction(times[(n-1)//2]+times[n//2], 2)
            medians[label] = median
            q1, _, q3 = statistics.quantiles(times, n=4, method="inclusive")
            line["outils"][label] = dict(n=n, mediane_ns=float(median), q1_ns=q1, q3_ns=q3,
                min_ns=min(times), max_ns=max(times), rss_max_octets=max(r["rss_octets"] for r in data),
                debit_sorties_Mio_s=size*count*1e9/float(median))
        for label in binaries:
            ratio = medians[label]/medians["GNU"]
            line["outils"][label].update(rapport_exact=str(ratio), rapport=float(ratio),
                                         conforme=ratio<=Fraction(11,10))
        result.append(line)
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie", type=Path)
    p.add_argument("--ancien", type=Path, required=True)
    p.add_argument("--nouveau", type=Path)
    p.add_argument("--tours", type=int, default=24)
    p.add_argument("--graine", type=int, required=True)
    a = p.parse_args()
    binaries = {"Ancien": a.ancien.resolve(), "GNU": GNU}
    if a.nouveau:
        binaries["Nouveau"] = a.nouveau.resolve()
    orders = list(itertools.permutations(binaries))
    if a.tours < 6 or a.tours % len(orders):
        p.error("Il faut un multiple du nombre de permutations et au moins six tours.")
    rng = random.Random(a.graine)
    ordering = {}
    for case in CASES:
        ordering[case] = []
        for _ in range(a.tours//len(orders)):
            block = orders.copy()
            rng.shuffle(block)
            ordering[case].extend(block)
    rows = []
    states = []
    meta = dict(machine=platform.platform(), graine=a.graine, tours=a.tours,
                executables={k:{"path":str(v),"sha256":sha(v)} for k,v in binaries.items()})
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as folder:
        root = Path(folder)
        block = random.Random(27).randbytes(MIO)
        inputs = {}
        for size in (1,8,32):
            source = root/f"corpus-{size}"
            with source.open("wb") as f:
                for _ in range(size):
                    f.write(block)
            inputs[size] = (source, sha(source))
        for turn in range(-2,a.tours):
            states.append(load_snapshot())
            cases = CASES.copy()
            rng.shuffle(cases)
            for size,count in cases:
                source, expected = inputs[size]
                dests = [root/f"sortie{i}" for i in range(count)]
                order = ordering[size,count][turn] if turn>=0 else orders[turn]
                for position,label in enumerate(order):
                    measurement = sample(binaries[label], source, dests, expected)
                    rows.append(dict(tour=turn, mio=size, sorties=count, outil=label,
                                     position=position, **measurement))
        states.append(load_snapshot())
    result = summary(rows,binaries)
    with a.sortie.open("x") as out:
        json.dump(dict(**meta, charge=states, mesures=rows, resume=result),out,indent=1)
        out.write("\n")
    for case in result:
        print(case["mio"],case["sorties"],[(k,round(v["mediane_ns"]/1e6,3),v["rapport_exact"],
              v["conforme"]) for k,v in case["outils"].items()],flush=True)

    if a.nouveau and any(not c["outils"]["Nouveau"]["conforme"] for c in result):
        raise SystemExit(1)

if __name__=="__main__":
    main()
