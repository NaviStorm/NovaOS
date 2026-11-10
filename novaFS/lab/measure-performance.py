#!/usr/bin/env python3
"""Rejoue le même harnais sur le moteur courant ou sur une extraction P3 vierge."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tempfile

LAB = Path(__file__).resolve().parent
TARGETS = {"aarch64": 0x0100000C, "x86_64": 0x01000007}
OVERLAY = ("novafs-bench.rs", "heap_meter.rs", "performance.rs")


def measure(binary, sector, scenario):
    cmd = ["/usr/bin/time", "-l", binary, str(sector), scenario]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=1200)
    if result.returncode:
        raise RuntimeError(f"{scenario} : {result.stderr[-2400:]}")
    rss = re.search(r"(\d+)\s+maximum resident set size", result.stderr)
    if not rss:
        raise RuntimeError("RSS Darwin manquante")
    lines = result.stdout.splitlines()
    if scenario == "complet":
        verify_barriers(lines)
    return {"secteur": sector, "scenario": scenario, "rss_octets": int(rss[1]),
            "journal": lines, "time_l": result.stderr.splitlines()}


def verify_barriers(lines):
    names = {"lecture_seq64k", "lecture_aleatoire4k", "ecriture_seq64k", "ecriture_aleatoire4k"}
    expected_keys = {(name, str(series)) for name in names for series in range(1, 6)}
    observed = {}
    checked = set()
    for line in lines:
        parts = line.split("\t")
        if parts[0] in names:
            key = (parts[0], parts[1])
            operations = 512 if "seq64k" in parts[0] else 256
            if int(parts[2]) != operations:
                raise RuntimeError(f"nombre d’opérations inattendu : {key}")
            expected = operations if parts[0].startswith("ecriture") else 0
            if key not in expected_keys or key in observed or int(parts[11]) != 2 * expected:
                raise RuntimeError(f"série ou nombre de flushs inattendu : {key}")
            observed[key] = expected
        elif parts[0] == "instrumentation" and parts[1] in names:
            key = (parts[1], parts[2])
            if key not in observed or key in checked:
                raise RuntimeError(f"instrumentation inattendue : {key}")
            expected = observed[key]
            if int(parts[3]) != expected or int(parts[4]) != expected:
                raise RuntimeError(f"nombre de F1/F2 inattendu : {parts[1]}")
            checked.add(key)
    if checked != expected_keys:
        raise RuntimeError("séries de référence incomplètes")


def build(source, arch, reference, directory):
    # Le moteur d'archive n'est jamais réécrit. Seul le banc instrumenté est
    # greffé sur la copie privée ; les trois sources sont identiques sur P3/P4.
    source = source.resolve()
    temp = directory / "novaFS"
    shutil.copytree(source, temp, ignore=shutil.ignore_patterns("target", "__pycache__", ".git"))
    for name in OVERLAY:
        shutil.copyfile(LAB / name, temp / "lab" / name)
    if reference:
        manifest = temp / "Cargo.toml"
        text = manifest.read_text()
        if 'version = "0.9.3"' not in text:
            raise ValueError("--reference exige un paquet NovaFS P3 0.9.3")
        if "bench-coordinator = " not in text:
            manifest.write_text(text.replace("[features]", "[features]\nbench-coordinator = []", 1))
    target = arch + "-apple-darwin"
    cmd = [temp / "lab/run", "build", "--offline", "--bin", "novafs-bench"]
    if not reference:
        cmd += ["--features", "bench-coordinator"]
    env = {**os.environ, "CARGO_TARGET_DIR": str(directory / "cible"), "NOVAFS_TARGET": target}
    subprocess.run(cmd, env=env, check=True)
    exe = directory / "cible" / target / "release" / "novafs-bench"
    with exe.open("rb") as stream:
        if stream.read(8) != bytes.fromhex("cffaedfe") + struct.pack("<I", TARGETS[arch]):
            raise RuntimeError("architecture Mach-O inattendue")
    return exe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch", required=True, choices=TARGETS)
    parser.add_argument("--reference", type=Path, help="dossier novaFS/ du commit 1510eca")
    parser.add_argument("--charge", choices=("toutes", "complet", "profil", "coordinateur"), default="toutes")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("macOS requis")
    if args.charge == "coordinateur" and args.reference:
        parser.error("la référence P3 ne comporte pas de coordinateur transactionnel")
    source = args.reference or LAB.parent
    with tempfile.TemporaryDirectory(prefix="novafs-performance-") as temporary:
        exe = build(source, args.arch, args.reference is not None, Path(temporary))
        cases = []
        if args.charge in ("toutes", "complet"):
            cases.append(measure(exe, 4096 if args.arch == "aarch64" else 512, "complet"))
        for sector in (512,4096):
            if args.charge in ("toutes", "profil"):
                cases.append(measure(exe, sector, "profil"))
            if not args.reference and args.charge in ("toutes", "coordinateur"):
                cases.append(measure(exe, sector, "coordinateur"))
        print(json.dumps({
            "architecture": args.arch, "moteur": "P3" if args.reference else "P4",
            "sha256_binaire": hashlib.sha256(exe.read_bytes()).hexdigest(), "campagnes": cases,
        }, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
