#!/usr/bin/env python3
"""Mesure du banc P3 de release sur des fichiers ordinaires temporaires macOS."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tempfile

LAB = Path(__file__).resolve().parent
CPUS = {"aarch64": 0x0100000C, "x86_64": 0x01000007}
PHASES = ("lecture_seq64k", "ecriture_seq64k", "lecture_aleatoire4k", "ecriture_aleatoire4k")


def values(line):
    return {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", line)}


def measure(binary, sector, mode):
    result = subprocess.run(["/usr/bin/time", "-l", binary, str(sector), mode],
                            text=True, capture_output=True, timeout=1200)
    if result.returncode:
        raise RuntimeError(f"{mode}, secteur {sector} : {result.stderr[-2400:]}")
    lines = result.stdout.splitlines()
    rss = re.search(r"(\d+)\s+maximum resident set size", result.stderr)
    if not rss:
        raise RuntimeError("RSS Darwin indisponible pour ce processus")
    data = {"secteur": sector, "rss_octets": int(rss[1])}
    for prefix in ("pile_principale=", "pic_malloc_octets="):
        data.update(values(next(line for line in lines if line.startswith(prefix))))
    if mode == "reservations":
        # La première exécution chauffe le runtime ; conserver la seconde.
        plans = [values(line) for line in lines if line.startswith("plan ")]
        tickets = [values(line) for line in lines if line.startswith("tickets ")]
        catalogs = [values(line) for line in lines if line.startswith("catalogue ")]
        assert len(plans) == 4 and len(tickets) == len(catalogs) == 2
        data.update(plans=plans[-2:], tickets=tickets[-1], catalogue=catalogs[-1],
                    restitution=values(next(x for x in lines if x.startswith("restitution_generale "))))
    else:
        header = next(x.split("\t") for x in lines if x.startswith("phase\t"))
        series = {}
        for phase in PHASES:
            rows = [dict(zip(header[1:], map(int, line.split("\t")[1:]), strict=True))
                    for line in lines if line.startswith(phase + "\t")]
            if len(rows) != 5:
                raise RuntimeError(f"séries incomplètes : {phase}")
            series[phase] = rows
            for row in rows:
                expected = 2 * row["operations"] if phase.startswith("ecriture") else 0
                if row["flushs"] != expected:
                    raise RuntimeError(f"nombre de barrières inattendu : {phase}")
        data["series"] = series
        data["pression"] = next(line for line in lines if line.startswith("pression\t"))
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch", required=True, choices=CPUS)
    opts = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("macOS est requis")
    target = opts.arch + "-apple-darwin"
    with tempfile.TemporaryDirectory(prefix="novafs-espace-") as tmp:
        env = {**os.environ, "CARGO_TARGET_DIR": tmp, "NOVAFS_TARGET": target}
        subprocess.run([LAB / "run", "build", "--offline", "--bin", "novafs-bench"], env=env, check=True)
        binary = Path(tmp) / target / "release" / "novafs-bench"
        with binary.open("rb") as handle:
            if handle.read(8) != bytes.fromhex("cffaedfe") + struct.pack("<I", CPUS[opts.arch]):
                raise RuntimeError("type Mach-O inattendu")
        data = {"architecture": opts.arch, "sha256_banc": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "reservations": [measure(binary, s, "reservations") for s in (512, 4096)],
                "complet": measure(binary, 4096 if opts.arch == "aarch64" else 512, "complet")}
        print(json.dumps(data, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
