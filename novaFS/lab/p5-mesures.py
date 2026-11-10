#!/usr/bin/env python3
"""Banc apparié du pool sur fichiers temporaires macOS. Sources P4.1 extraites de Git."""
import argparse
import csv
import gzip
import io
import json
import math
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile

LAB = Path(__file__).resolve().parent
REPO = LAB.parent.parent
REF = "b865c58"
CPUS = {"aarch64": 0x0100000C, "x86_64": 0x01000007}


def build(arch, phase, work):
    root = REPO if phase == "P5" else work / REF
    if not root.exists():
        root.mkdir()
        archive = subprocess.check_output(["git", "-C", REPO, "archive", REF, "novaFS"])
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(root, filter="data")
    target = f"{arch}-apple-darwin"
    out = work / f"target-{phase}"
    cargo = subprocess.run([root / "novaFS/lab/run", "build", "--lib", "--offline", "--message-format=json"],
                           env=dict(os.environ, NOVAFS_TARGET=target, CARGO_TARGET_DIR=str(out)),
                           check=True, stdout=subprocess.PIPE)
    artifacts = {}
    for line in cargo.stdout.splitlines():
        r = json.loads(line)
        for f in r.get("filenames", []):
            if f.endswith(".rlib"):
                artifacts[r["target"]["name"]] = f
    binary = work / f"mesure-{arch}-{phase}"
    args = ["rustup", "run", "stable", "rustc", "--edition", "2024", "-O", "--target", target,
            "-L", f"dependency={out / target / 'release/deps'}", "--extern", f"novafs={artifacts['novafs']}"]
    if phase == "P5":
        args += ["--cfg", "p5", "--extern", f"tempfile={artifacts['tempfile']}"]
    subprocess.run([*args, LAB / "p5_mesures.rs", "-o", binary], check=True)
    with binary.open("rb") as f:
        assert f.read(8) == bytes.fromhex("cffaedfe") + CPUS[arch].to_bytes(4, "little")
    return binary


def quantiles(values):
    if not values:
        return None
    values = sorted(values)
    n = len(values)
    return dict(p50_ns=values[math.ceil(.5*n)-1], p95_ns=values[math.ceil(.95*n)-1],
                p99_ns=values[math.ceil(.99*n)-1], max_ns=values[-1])


def summary(runs):
    durations = sorted(r[0] for r in runs)
    n = len(runs)
    time = sum(durations)
    useful = sum(r[1] for r in runs)
    writes = sum(r[2] for r in runs)
    sync = sum(r[3] for r in runs)
    sync_time = sum(r[4] for r in runs)
    return dict(n=n, **quantiles(durations), total_ns=time,
                F1=quantiles([r[5] for r in runs if r[3]>=2]),
                F2=quantiles([r[6] for r in runs if r[3]>=2]),
                utiles_octets=useful, ecritures_blocs=writes, synchronisations=sync,
                temps_sync_ns=sync_time, sync_moy_ns=sync_time/sync if sync else None,
                debit_mio_s=useful*1e9/(2**20*time) if useful else None,
                amplification=writes*4096/useful if useful and writes else None)


def measure(binary, sector, basename):
    result = subprocess.run(["/usr/bin/time", "-l", binary, str(sector)],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr)
    rss = int(re.search(r"(\d+)\s+maximum resident set size", result.stderr)[1])
    records = {}
    memory = None
    with gzip.open(str(basename)+".csv.gz", "wt", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["operation", "mur_ns", "utiles_octets", "ecritures_blocs", "flushs", "temps_flush_ns", "F1_ns", "F2_ns"])
        for line in result.stdout.splitlines():
            fields = line.split()
            if fields[0] == "sample":
                operation = fields[1]
                record = list(map(int, fields[2:]))
                writer.writerow([operation, *record])
                records.setdefault(operation, []).append(record)
            elif fields[0] == "memory":
                heap, work_stack, main_stack = map(int, fields[1:])
                memory = dict(tas_pic=heap, pile_travail=work_stack, pile_main_hote=main_stack,
                              tas_et_pile_travail=heap+work_stack, rss_pic=rss)
    assert memory and records["read"] and records["write"]
    stats = {"secteur": sector, "memoire": memory,
             "operations": {kind: summary(samples) for kind,samples in records.items()}}
    Path(str(basename)+".json").write_text(json.dumps(stats, indent=2, ensure_ascii=False)+"\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--sortie", required=True, type=Path)
    ap.add_argument("--phase", choices=["P4.1", "P5", "toutes"], default="toutes")
    args = ap.parse_args()
    args.sortie.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="novafs-comparatif-") as tmp:
        for arch in CPUS:
            phases = ["P4.1", "P5"] if args.phase=="toutes" else [args.phase]
            binaries = {phase: build(arch, phase, Path(tmp)) for phase in phases}
            for sector in [512,4096]:
                order = phases if sector == 512 else phases[::-1]
                for phase in order:
                    measure(binaries[phase], sector, args.sortie/f"{arch}-{phase}-{sector}")


if __name__ == "__main__":
    main()
