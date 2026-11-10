#!/usr/bin/env python3
"""Banc P6 apparié sur des fichiers ordinaires macOS, avec oracle des octets et barrières."""
import argparse
from collections import defaultdict
from datetime import datetime, timezone
import gzip
import json
import math
import os
from pathlib import Path
import platform
import re
import runpy
import subprocess
import tempfile

LAB = Path(__file__).resolve().parent
OUTILS = runpy.run_path(str(LAB / "p51-validation.py"))
MATRICE = runpy.run_path(str(LAB / "p51-matrice.py"))
REF = "3ca74ee"
ARCHS = ("aarch64", "x86_64")
SECTEURS = (512, 4096)
MODES = ("P5.1", "P6-none", "P6-lz4")


def build(root, work, arch, phase):
    target = f"{arch}-apple-darwin"
    probes = phase.endswith("sondes")
    out = work / ("target6" if phase.startswith("P6") else "target51")
    args = [root / "lab/run", "build", "--offline", "--lib", "--message-format=json"]
    if probes:
        args += ["--features", "latency-probes"]
    env = dict(os.environ, NOVAFS_TARGET=target, CARGO_TARGET_DIR=str(out))
    records = subprocess.check_output(args, env=env).splitlines()
    libs = {}
    for line in records:
        msg = json.loads(line)
        for name in msg.get("filenames", []):
            if name.endswith(".rlib"):
                libs[msg["target"]["name"]] = name
    exe = work / f"mesure-{arch}-{phase}"
    rustc = ["rustup", "run", "stable", "rustc", "--edition=2024", "-O", "--target", target,
           "-L", f"dependency={out/target/'release/deps'}", LAB / "p6_mesures.rs", "-o", exe]
    for lib in ("novafs", "serde_json"):
        rustc += ["--extern", f"{lib}={libs[lib]}"]
    if phase.startswith("P6"):
        rustc += ["--cfg", "p6"]
    if probes:
        rustc += ["--cfg", "sondes"]
    subprocess.run(rustc, check=True)
    assert exe.read_bytes()[:8] == bytes.fromhex("cffaedfe") + MATRICE["CPUS"][arch].to_bytes(4, "little")
    return exe


def quantile(v, q):
    return v[math.ceil(len(v) * q)-1]


def synthese(rows):
    groups = defaultdict(list)
    for r in rows:
        if r["type"] == "appel":
            groups[tuple(r[k] for k in ("arch", "secteur", "phase", "profil", "fichier", "operation", "partielle"))].append(r)
    def total(rr, k):
        return sum(r[k] for r in rr)
    out = []
    for (a, sec, phase, profil, taille, op, partial), rr in sorted(groups.items()):
        ns = sorted(r["mur_ns"] for r in rr)
        size = total(rr, "octets")
        out.append(dict(arch=a, secteur=sec, phase=phase, profil=profil, fichier=taille, operation=op,
                        octets_partiels=partial, n=len(rr), p50_ns=quantile(ns, .5), p95_ns=quantile(ns, .95),
                        p99_ns=quantile(ns, .99), max_ns=max(ns),
                        debit_mio_s=size / sum(ns) * 1e9 / 2**20 if size else None,
                        amplification=4096 * total(rr, "ecritures") / size if size and total(rr, "ecritures") else None,
                        **{k:total(rr, k) for k in ("octets", "mur_ns", "cpu_ns", "compression_ns", "decompression_ns",
                            "lectures", "ecritures", "lectures_ns", "ecritures_ns", "flushs", "flush_ns", "F1_ns", "F2_ns")}))
    return out


def save(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


def verifier(rows, repetitions):
    """Les décalages du client et les octets utiles doivent être appariés entre toutes les phases."""
    trajectoires = defaultdict(list)
    for r in rows:
        if r["type"] == "appel" and not r["phase"].endswith("sondes"):
            cle = (r["arch"], r["secteur"], r["repetition"], r["phase"])
            trajectoires[cle].append(tuple(r[k] for k in ("profil", "fichier", "operation", "n", "offset", "octets")))
    assert len(trajectoires) == len(ARCHS) * len(SECTEURS) * len(MODES) * repetitions
    for (arch, sec, rep, phase), panel in trajectoires.items():
        assert panel == trajectoires[(arch, sec, rep, "P5.1")], (arch, sec, rep, phase)
    for arch in ARCHS:
        for sec in SECTEURS:
            sondes = [r for r in rows if r["arch"] == arch and r["secteur"] == sec
                      and r["type"] == "appel" and r["phase"].endswith("sondes")]
            assert sum(r["compression_ns"] for r in sondes) > 0
            assert sum(r["decompression_ns"] for r in sondes) > 0


def historique(work):
    folder = work / REF / "novaFS"
    if not folder.exists():
        OUTILS["extraire"](REF, work)
    return folder


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--sortie", required=True, type=Path)
    ap.add_argument("--travail", type=Path, help="Conserver les cibles de compilation et les copies historiques.")
    ap.add_argument("--repetitions", type=int, default=2)
    args = ap.parse_args()
    assert args.repetitions >= 1
    args.sortie.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="novafs-p6-mesures-") as tmp:
        work = args.travail or Path(tmp)
        work.mkdir(parents=True, exist_ok=True)
        p51 = historique(work)
        binaries = {}
        report = dict(macos=platform.platform(), machine=platform.machine(),
                    cpu=MATRICE["command"]("sysctl", "-n", "machdep.cpu.brand_string"),
                    rustc=MATRICE["command"]("rustup", "run", "stable", "rustc", "-Vv"),
                    revision_p51=MATRICE["command"]("git", "rev-parse", REF),
                    p51=MATRICE["source_hash"](p51, REF), p6=MATRICE["source_hash"](LAB.parent, None),
                    sources_banc={f:MATRICE["fingerprint"](LAB / f) for f in ("p6_mesures.rs", "p6_donnees.rs", "heap_meter.rs", "thread_clock.rs", "latency_probes.rs")},
                    binaires={}, horloges={}, executions=[])
        for arch in ARCHS:
            for phase in ("P5.1", "P6", "P6-sondes"):
                b = build(p51 if phase == "P5.1" else LAB.parent, work, arch, phase)
                binaries[arch, phase] = b
                report["binaires"][f"{arch}/{phase}"] = MATRICE["fingerprint"](b)
            report["horloges"][arch] = json.loads(subprocess.check_output([binaries[arch, "P6"], "horloge"]))
        assert report["p6"] == MATRICE["source_hash"](LAB.parent, None), "sources modifiées pendant la compilation"
        rows = []
        trials = []
        for rep in range(args.repetitions):
            for arch in (ARCHS if rep % 2 == 0 else reversed(ARCHS)):
                for sector in (SECTEURS if rep % 2 == 0 else reversed(SECTEURS)):
                    rotation = (ARCHS.index(arch) + SECTEURS.index(sector)) % len(MODES)
                    modes = MODES[rotation:] + MODES[:rotation]
                    for mode in (modes if rep % 2 == 0 else modes[::-1]):
                        trials.append((arch, sector, mode, rep))
        for arch in ARCHS:
            for sector in SECTEURS:
                trials.append((arch, sector, "P6-lz4-sondes", 0))
        for arch, sector, phase, rep in trials:
            binary = binaries[arch, "P5.1" if phase == "P5.1" else ("P6-sondes" if phase.endswith("sondes") else "P6")]
            start = datetime.now(timezone.utc).isoformat()
            p = subprocess.run(["/usr/bin/time", "-l", binary, str(sector), "lz4" if "lz4" in phase else "none", str(rep)],
                             capture_output=True, text=True)
            if p.returncode:
                raise RuntimeError(p.stderr + "\n" + p.stdout[-200:])
            heap, stack = map(int, re.search(r"^memoire (\d+) (\d+)$", p.stderr, re.M).groups())
            rss = int(re.search(r"(\d+)\s+maximum resident set size", p.stderr)[1])
            key = dict(arch=arch, secteur=sector, phase=phase, repetition=rep + 1)
            records = [dict(json.loads(line), **key) for line in p.stdout.splitlines()]
            for r in records:
                if r["type"] == "appel":
                    r["partielle"] = r["octets"] if r["operation"] == "partielle_clone" else 0
            assert heap + stack <= 7 * 2**20, (key, heap, stack)
            assert any(r["type"] == "espace" for r in records)
            rows.extend(records)
            report["executions"].append(dict(**key, debut_utc=start, fin_utc=datetime.now(timezone.utc).isoformat(),
                      tas_pic=heap, pile_travail=stack, rss_pic=rss))
            print(f"{arch} secteur={sector} {phase} répétition={rep+1} enregistrements={len(records)}", flush=True)
        verifier(rows, args.repetitions)
        with gzip.open(args.sortie / "echantillons.jsonl.gz", "wt") as f:
            for r in rows:
                f.write(json.dumps(r, separators=(",", ":")) + "\n")
        with gzip.open(args.sortie / "distributions.json.gz", "wt") as f:
            json.dump(synthese(rows), f, separators=(",", ":"))
        save(args.sortie / "campagnes.json", report)
        save(args.sortie / "espace.json", [r for r in rows if r["type"] == "espace"])


if __name__ == "__main__":
    main()
