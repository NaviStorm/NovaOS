#!/usr/bin/env python3
"""P4.1 : préparation des images P4 et comparaison de lectures indépendantes sur macOS."""
import argparse
import csv
import gzip
import hashlib
import itertools
import json
import os
from pathlib import Path
import platform
import re
import shutil
import struct
import subprocess
import time

LAB = Path(__file__).resolve().parent
ROOT = LAB.parent.parent
TARGETS = {"aarch64": 0x0100000C, "x86_64": 0x01000007}
VERSIONS = {"P3": "1510eca", "P4": "0a89d23", "P4.1": None}
FIELDS = "total_ns cpu_ns lecture_ns controle_liberation_ns fichier_ns lectures touches absences evictions allocations".split()
META = ["campagne", "ordre", "architecture", "moteur", "cache_hote", "phase", "serie", "appel"]


def sha(path):
    h = hashlib.sha256()
    with path.open("rb") as inp:
        for page in iter(lambda: inp.read(1 << 20), b""):
            h.update(page)
    return h.hexdigest()


def capture(*cmd):
    return subprocess.check_output(cmd, text=True, cwd=ROOT).strip()


def save(path, obj):
    path.write_text(json.dumps(obj, ensure_ascii=False, separators=(",", ":")) + "\n")


def build(work, label):
    source = work / "source" / label
    source.mkdir(parents=True, exist_ok=True)
    ref = VERSIONS[label]
    if ref:
        archive = subprocess.Popen(["git", "archive", ref, "novaFS"], cwd=ROOT, stdout=subprocess.PIPE)
        unpack = subprocess.run(["tar", "-x", "-C", source], stdin=archive.stdout)
        archive.stdout.close()
        if archive.wait() or unpack.returncode:
            raise RuntimeError(f"archive {ref}")
    else:
        shutil.copytree(LAB.parent, source / "novaFS", dirs_exist_ok=True, ignore=shutil.ignore_patterns(
            "target", "docs", "__pycache__", ".git"))
    pkg = source / "novaFS"
    for f in ("latency.rs", "heap_meter.rs", "thread_clock.rs"):
        shutil.copyfile(LAB / f, pkg / "lab" / f)
    manifest_path = pkg / "Cargo.toml"
    if ref:
        manifest_path.write_text(manifest_path.read_text().replace("[features]", "[features]\nlatency-probes = []", 1))
    with manifest_path.open("a") as manifest:
        manifest.write('\n[[bin]]\nname = "novafs-latency"\npath = "lab/latency.rs"\n')
    build_script = pkg / "build.rs"
    script = build_script.read_text()
    line = 'println!("cargo::rustc-link-arg-bin=novafs-bench=-Wl,-stack_size,0x200000");'
    if script.count(line) != 1:
        raise RuntimeError("configuration Darwin de la pile de référence introuvable")
    build_script.write_text(script.replace(line, line + "\n        " + line.replace("novafs-bench", "novafs-latency")))
    binaries = {}
    for arch, magic in TARGETS.items():
        target = arch + "-apple-darwin"
        dest = work / "cibles" / label
        env = dict(os.environ, NOVAFS_TARGET=target, CARGO_TARGET_DIR=str(dest))
        subprocess.run([pkg / "lab/run", "build", "--offline", "--bin", "novafs-latency"],
                       cwd=pkg, env=env, check=True)
        exe = dest / target / "release/novafs-latency"
        with exe.open("rb") as f:
            if f.read(8) != bytes.fromhex("cffaedfe") + struct.pack("<I", magic):
                raise RuntimeError(f"Mach-O {arch} attendu")
        inputs = [*pkg.joinpath("src").rglob("*.rs"), pkg / "lab/device.rs",
                  *(pkg / "lab" / name for name in ("heap_meter.rs", "latency.rs", "thread_clock.rs")),
                  pkg / "Cargo.toml", pkg / "Cargo.lock", pkg / "build.rs"]
        provenance = hashlib.sha256()
        for path in sorted(inputs):
            provenance.update((str(path.relative_to(pkg)) + "\0" + sha(path) + "\n").encode())
        binaries[arch] = {"chemin": str(exe.relative_to(work)), "sha256": sha(exe),
                          "sha256_sources_harnais_et_moteur": provenance.hexdigest()}
    return binaries


def prepare(work):
    work.mkdir(parents=True, exist_ok=True)
    if any(work.iterdir()):
        raise ValueError("la préparation exige un répertoire de travail vide")
    info = {"versions": VERSIONS, "binaires": {}, "images": {},
            "revision_initiale": capture("git", "rev-parse", "HEAD"),
            "rust": capture("rustup", "run", "stable", "rustc", "-vV"),
            "macOS": capture("sw_vers"), "machine": platform.machine()}
    for label in VERSIONS:
        info["binaires"][label] = build(work, label)
    imgs = work / "images"
    imgs.mkdir()
    subprocess.run([work / info["binaires"]["P4"]["aarch64"]["chemin"], "prepare", imgs], check=True)
    info["images"] = {f.name: sha(f) for f in sorted(imgs.glob("serie-*.img"))}
    assert len(info["images"]) == 5
    save(work / "preparation.json", info)


def rebuild(work):
    path = work / "preparation.json"
    info = json.loads(path.read_text())
    for label in VERSIONS:
        info["binaires"][label] = build(work, label)
    save(path, info)


def distribution(values):
    values = sorted(values)
    def percentile(p):
        return values[(len(values) * p + 99) // 100 - 1]
    return {"n": len(values), "p50": percentile(50), "p95": percentile(95),
            "p99": percentile(99), "max": values[-1]}


def summarize(rows):
    wall = [r[0] for r in rows]
    return {"duree_appel_ns": distribution(wall), "cpu_thread_ns": distribution(r[1] for r in rows),
            "reliquat_mur_cpu_ns": distribution(max(0, r[0] - r[1]) for r in rows),
            "lecture_moteur_ns": distribution(r[2] for r in rows),
            "controle_liberation_ns": distribution(r[3] for r in rows),
            "fichier_ns": distribution(r[4] for r in rows),
            **{name: sum(r[i] for r in rows) for i, name in enumerate(FIELDS) if i >= 5}}


def result(exe, imgs, arch, policy, ids, writer):
    sector = 4096 if arch == "aarch64" else 512
    started = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    load_before = os.getloadavg()
    proc = subprocess.run(["/usr/bin/time", "-l", exe, imgs, str(sector), policy],
                          text=True, capture_output=True, timeout=900)
    if proc.returncode:
        raise RuntimeError(proc.stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", proc.stderr)
    if not rss:
        raise RuntimeError("RSS Darwin absente")
    calibration = next((line.removeprefix("calibration_cpu=") for line in proc.stderr.splitlines()
                        if line.startswith("calibration_cpu=")), None)
    if calibration is None:
        raise ValueError("calibration CPU Darwin absente")
    report = {"calibration_cpu": json.loads(calibration), "rss_octets": int(rss[1]), "secteur": sector, "phases": {},
              "debut_UTC": started, "fin_UTC": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "charge_hote_avant_1_5_15_min": load_before, "charge_hote_apres_1_5_15_min": os.getloadavg()}
    pooled = {"froid_32m": [], "chaud_512k": []}
    lines = iter(proc.stdout.splitlines())
    for series in range(1, 6):
        for phase, all_rows in pooled.items():
            head = next(lines).split()
            if head[:3] != ["phase", str(series), phase]:
                raise ValueError(head)
            rows = [list(map(int, next(lines).split())) for _ in range(512)]
            if any(len(r) != len(FIELDS) or r[0] != r[2] + r[3] or r[1] <= 0 for r in rows):
                raise ValueError("échantillon invalide")
            for i, row in enumerate(rows):
                writer.writerow([*ids, phase, series, i, *row])
            all_rows.extend(rows)
            group = report["phases"].setdefault(phase, {"series": []})
            group["series"].append({"serie": series, "boucle_ns": int(head[3]), "pic_tas": int(head[4]),
                                    **summarize(rows)})
    tag, heap, main, worker = next(lines).split()
    if tag != "memoire" or list(lines):
        raise ValueError("fin du rapport invalide")
    report.update(tas_octets=int(heap), pile_principale=int(main), pile_travail=int(worker))
    for phase, all_rows in pooled.items():
        grp = report["phases"][phase]
        ns = sum(s["boucle_ns"] for s in grp["series"])
        grp.update(**summarize(all_rows), debit_mio_s=len(all_rows) * 0.0625 * 1e9 / ns,
                   pire_p99_serie_ns=max(s["duree_appel_ns"]["p99"] for s in grp["series"]))
    return report


def campaigns(work, out, count):
    info = json.loads((work / "preparation.json").read_text())
    for label, arches in info["binaires"].items():
        for entry in arches.values():
            if sha(work / entry["chemin"]) != entry["sha256"]:
                raise ValueError(f"binaire modifié : {label}")
    for name, digest in info["images"].items():
        if sha(work / "images" / name) != digest:
            raise ValueError(f"image modifiée : {name}")
    out.mkdir(parents=True, exist_ok=True)
    raw = out / "lectures.csv.gz"
    summary = out / "lectures.json"
    if raw.exists() or summary.exists():
        raise ValueError("fichiers de campagne déjà présents")
    run = dict(info, methodologie="rang supérieur, p99 du regroupement de requêtes individuelles",
               debut=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), campagnes=[])
    run["materiel"] = {key: capture("sysctl", "-n", key) for key in
                       ("hw.model", "machdep.cpu.brand_string", "hw.ncpu", "hw.memsize")}
    orders = list(itertools.permutations(VERSIONS))
    with gzip.open(raw, "xt", newline="", compresslevel=6) as stream:
        writer = csv.writer(stream)
        writer.writerow(META + FIELDS)
        for i in range(count):
            # Couples inverses ABC/CBA, BCA/ACB, CAB/BAC : chacune des trois
            # versions occupe deux fois chacune des trois positions sur six campagnes.
            order = [orders[j] for j in (0, 5, 3, 1, 4, 2)][i % 6]
            archs = list(TARGETS) if i % 2 == 0 else list(reversed(TARGETS))
            for arch in archs:
                policies = ("buffered", "nocache") if (i + list(TARGETS).index(arch)) % 2 == 0 else ("nocache", "buffered")
                for policy in policies:
                    for pos, version in enumerate(order):
                        exe = work / info["binaires"][version][arch]["chemin"]
                        ids = [i+1, pos+1, arch, version, policy]
                        print(f"campagne {i+1}/{count} {arch} {policy} {version}", flush=True)
                        metrics = result(exe, work / "images", arch, policy, ids, writer)
                        run["campagnes"].append(dict(zip(META, ids), **metrics))
    run.update(fin=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
               sha256_csv_gz=sha(raw), lignes=count*len(VERSIONS)*len(TARGETS)*2*2*5*512)
    save(summary, run)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("action", choices=("preparer", "recompiler", "mesurer"))
    p.add_argument("--travail", required=True, type=Path)
    p.add_argument("--sortie", type=Path)
    p.add_argument("--campagnes", type=int, default=6, choices=range(1, 31))
    args = p.parse_args()
    if platform.system() != "Darwin":
        p.error("macOS requis")
    if args.action == "preparer":
        prepare(args.travail.resolve())
    elif args.action == "recompiler":
        rebuild(args.travail.resolve())
    elif args.sortie:
        campaigns(args.travail.resolve(), args.sortie.resolve(), args.campagnes)
    else:
        p.error("--sortie est requis pour mesurer")


if __name__ == "__main__":
    main()
