#!/usr/bin/env python3
"""Contrôle sur les mêmes images et bibliothèques, allocateur System et chrono mural seuls."""
import argparse
from collections import defaultdict
import gzip
import json
from pathlib import Path
import runpy
from statistics import median
import struct
import subprocess
import time

LAB = Path(__file__).resolve().parent
common = runpy.run_path(str(LAB / "measure-latency.py"))
sha, dist = common["sha"], common["distribution"]
ARCHES = common["TARGETS"]
PHASES = ("froid_32m", "chaud_512k")
ORDERS = (("P3", "P4", "P4.1"), ("P4", "P4.1", "P3"), ("P4.1", "P3", "P4"))


def build(work, prepared):
    folder = work / "temoin-bin"
    folder.mkdir(exist_ok=True)
    built = {}
    for label, arches in prepared["binaires"].items():
        for arch, measured in arches.items():
            reference = work / measured["chemin"]
            if sha(reference) != measured["sha256"]:
                raise ValueError(f"binaire instrumenté modifié : {reference}")
            deps = reference.parent / "deps"
            rlib, = deps.glob("libnovafs-*.rlib")
            exe = folder / f"temoin-{label}-{arch}"
            cmd = ["rustup", "run", "stable", "rustc", "--edition=2024", "--target", f"{arch}-apple-darwin",
                   "-C", "opt-level=3", "-C", "overflow-checks=yes", "-C", "link-arg=-Wl,-stack_size,0x200000",
                   "-L", f"dependency={deps}", "--extern", f"novafs={rlib}", LAB / "temoin.rs", "-o", exe]
            subprocess.run(cmd, check=True)
            with exe.open("rb") as f:
                if f.read(8) != bytes.fromhex("cffaedfe") + struct.pack("<I", ARCHES[arch]):
                    raise ValueError(f"Mach-O inattendu : {exe}")
            built[label, arch] = (exe, {"sha256_rlib": sha(rlib), "sha256_binaire": sha(exe),
                                        "sha256_binaire_instrumente": measured["sha256"]})
    return built


def measure(exe, images, arch):
    sector = 4096 if arch == "aarch64" else 512
    start = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    run = subprocess.run([exe, images, str(sector)], capture_output=True, text=True, timeout=300)
    if run.returncode:
        raise RuntimeError(f"échec du lecteur {exe} : {run.stderr[-2000:]}")
    lines = iter(run.stdout.splitlines())
    stacks = {}
    for name in ("pile_principale", "pile_travail"):
        key, value = next(lines).split()
        if key != name or int(value) <= 0:
            raise ValueError("taille pthread absente")
        stacks[name] = int(value)
    rounds = []
    for series in range(1, 6):
        for phase in PHASES:
            fields = next(lines).split()
            if fields[:3] != ["phase", str(series), phase]:
                raise ValueError("série de lecture incorrecte")
            loop, *samples = map(int, fields[3:])
            if len(samples) != 512 or min(samples) <= 0 or loop < sum(samples):
                raise ValueError("échantillons incomplets ou durée incohérente")
            rounds.append(dict(serie=series, phase=phase, boucle_ns=loop, echantillons_ns=samples))
    if list(lines):
        raise ValueError("mesures supplémentaires")
    return dict(debut_UTC=start, secteur=sector, **stacks, series=rounds)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--travail", required=True, type=Path, help="répertoire de measure-latency.py preparer")
    parser.add_argument("--sortie", required=True, type=Path)
    args = parser.parse_args()
    work = args.travail.resolve()
    out = args.sortie.resolve()
    prepared = json.loads((work / "preparation.json").read_text())
    images = work / "images"
    if prepared["images"].keys() != {f"serie-{i}.img" for i in range(1, 6)}:
        raise ValueError("cinq images requises")
    for name, digest in prepared["images"].items():
        if sha(images / name) != digest:
            raise ValueError(f"image différente : {name}")
    out.mkdir(parents=True, exist_ok=True)
    raw = out / "temoin.jsonl.gz"
    summary_path = out / "temoin.json"
    if raw.exists() or summary_path.exists():
        raise ValueError("le rapport témoin existe déjà")
    binaries = build(work, prepared)
    runs = []
    pooled = defaultdict(list)
    duration = defaultdict(int)
    with gzip.open(raw, "xt") as stream:
        for campaign, order in enumerate(ORDERS, 1):
            arches = list(ARCHES) if campaign % 2 else list(reversed(ARCHES))
            for arch in arches:
                for position, label in enumerate(order, 1):
                    print(f"témoin {campaign}/3 {arch} {label}", flush=True)
                    data = measure(binaries[label, arch][0], images, arch)
                    data.update(campagne=campaign, ordre=position, architecture=arch, moteur=label)
                    stream.write(json.dumps(data, separators=(",", ":")) + "\n")
                    phases = {}
                    for phase in PHASES:
                        subset = [s for s in data["series"] if s["phase"] == phase]
                        sample = [ns for s in subset for ns in s["echantillons_ns"]]
                        elapsed = sum(s["boucle_ns"] for s in subset)
                        phases[phase] = dict(**dist(sample), Mio_s=len(sample) * 0.0625e9 / elapsed)
                        key = arch, label, phase
                        duration[key] += elapsed
                        pooled[key].extend(sample)
                    runs.append({k: v for k, v in data.items() if k != "series"} | {"phases": phases})
    groups = [dict(architecture=a, moteur=version, phase=p, latence_ns=dist(s),
                   Mio_s=len(s) * 0.0625e9 / duration[a,version,p])
              for (a,version,p), s in sorted(pooled.items())]
    pairs = []
    for arch in ARCHES:
        for phase in PHASES:
            by_run = {(r["campagne"],r["moteur"]): r["phases"][phase]["p99"] for r in runs
                      if r["architecture"] == arch}
            for old,new in (("P3", "P4"), ("P4", "P4.1")):
                ratios = [by_run[c,new]/by_run[c,old] for c in range(1,4)]
                pairs.append(dict(architecture=arch, avant=old, apres=new, phase=phase,
                                  ratios_p99=ratios, mediane=median(ratios)))
    report = dict(images=prepared["images"], sha256_lecteur=sha(LAB/"temoin.rs"), sha256_brut=sha(raw),
                  binaires=[dict(moteur=label, architecture=arch, **hashes)
                            for (label,arch), (_,hashes) in binaries.items()],
                  machine=prepared["machine"], rust=prepared["rust"], macOS=prepared["macOS"],
                  campagnes=runs, groupes=groups, comparaisons=pairs)
    summary_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
