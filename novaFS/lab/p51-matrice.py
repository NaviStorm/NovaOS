#!/usr/bin/env python3
"""Campagnes FIFO appariées sur des fichiers Darwin anonymes. Tous les échantillons sont conservés."""
import argparse
import csv
import gzip
import hashlib
import io
import json
import math
import os
import platform
import re
import statistics
import subprocess
import tarfile
import tempfile
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path

LAB = Path(__file__).resolve().parent
REPO = LAB.parent.parent
PHASES = {"P4.1": "b865c58", "P5": "ae508ce", "P5.1": None}
CPUS = {"aarch64": 0x0100000C, "x86_64": 0x01000007}
CONDITIONS = ["chaud", "froid", "nocache"]
KEY = ["arch", "phase", "secteur", "cache", "topologie", "ordre", "taille", "operation"]
HEADER = ["topologie", "ordre", "taille", "operation", "indice", "offset", "prepare_ns", "mur_ns", "cpu_ns",
          "allocations", "lectures_donnees", "lectures_meta_fs", "lectures_carte", "lectures_catalogue",
          "ecritures_donnees", "ecritures_meta_fs", "ecritures_carte", "ecritures_catalogue",
          "temps_lecture_ns", "temps_ecriture_ns", "flushs", "temps_flush_ns", "F1_ns", "F2_ns"]


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def build(work, arch, phase):
    ref = PHASES[phase]
    root = REPO if ref is None else work / ref
    if not root.exists():
        tar_bytes = subprocess.check_output(["git", "-C", REPO, "archive", ref, "novaFS"])
        root.mkdir()
        with tarfile.open(fileobj=io.BytesIO(tar_bytes)) as tar:
            tar.extractall(root, filter="data")
    target = f"{arch}-apple-darwin"
    out = work / f"target-{phase}"
    inputs = build_inputs(work, arch, phase)
    env = dict(os.environ, NOVAFS_TARGET=target, CARGO_TARGET_DIR=str(out))
    proc = subprocess.run([root / "novaFS/lab/run", "build", "--offline", "--lib",
                           "--message-format=json"], env=env, check=True, capture_output=True)
    artifacts = {}
    for line in proc.stdout.splitlines():
        msg = json.loads(line)
        for filename in msg.get("filenames", []):
            if filename.endswith(".rlib"):
                artifacts[msg["target"]["name"]] = filename
    path = work / f"matrice-{arch}-{phase}"
    flags = ["--cfg", "shared"] if phase != "P4.1" else []
    args = ["rustup", "run", "stable", "rustc", "--edition", "2024", "-O", "--target", target, *flags,
            "-L", f"dependency={out / target / 'release/deps'}", LAB / "p51_matrice.rs", "-o", path]
    for lib in ("novafs", "tempfile", "serde_json"):
        args += ["--extern", f"{lib}={artifacts[lib]}"]
    subprocess.run(args, check=True)
    with path.open("rb") as f:
        assert f.read(8) == bytes.fromhex("cffaedfe") + CPUS[arch].to_bytes(4, "little")
    if inputs != build_inputs(work, arch, phase):
        raise RuntimeError("Les sources ont changé pendant la compilation.")
    manifest_path(path).write_text(json.dumps(dict(inputs=inputs, binaire=fingerprint(path)),
                                               sort_keys=True, indent=2) + "\n")
    return path


def fingerprint(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def source_hash(root, revision):
    digest = hashlib.sha256()
    files = sorted((root / "src").rglob("*.rs")) + sorted(root.glob("Cargo.*"))
    files += [root / "lab/device.rs", root / "lab/run", root / "build.rs"]
    historical = {}
    if revision:
        listing = subprocess.check_output(["git", "-C", REPO, "ls-tree", "-r", "-z", revision, "novaFS"])
        for line in listing.split(b"\0"):
            if line:
                obj, name = line.split(b"\t", 1)
                historical[name.decode().removeprefix("novaFS/")] = obj.split()[2].decode()
        if {p for p in historical if p.startswith("src/") and p.endswith(".rs")} != {
                str(p.relative_to(root)) for p in files if root / "src" in p.parents}:
            raise RuntimeError(f"Liste de sources différente de l'archive {revision}")
        git_hash = command("git", "-C", REPO, "rev-parse", "--show-object-format")
    for path in files:
        name = str(path.relative_to(root))
        content = path.read_bytes()
        if revision:
            blob = f"blob {len(content)}\0".encode() + content
            if hashlib.new(git_hash, blob).hexdigest() != historical.get(name):
                raise RuntimeError(f"{name} ne correspond pas à {revision}")
        digest.update(name.encode() + b"\0" + hashlib.sha256(content).digest())
    return digest.hexdigest()


def manifest_path(binary):
    return binary.parent / (binary.name + ".provenance.json")


def build_inputs(work, arch, phase):
    ref = PHASES[phase]
    root = REPO if ref is None else work / ref
    revision = command("git", "-C", REPO, "rev-parse", ref) if ref else None
    return dict(arch=arch, phase=phase, moteur=source_hash(root / "novaFS", revision), revision=revision,
                banc={name: fingerprint(LAB / name) for name in
                      ["p51_matrice.rs", "heap_meter.rs", "thread_clock.rs"]},
                rustc=command("rustup", "run", "stable", "rustc", "-vV"))


def verify_binary(work, arch, phase, path):
    stamp = manifest_path(path)
    if not stamp.is_file():
        raise RuntimeError(f"Provenance absente pour {path}. Recompiler avec --preparer.")
    actual = dict(inputs=build_inputs(work, arch, phase), binaire=fingerprint(path))
    if json.loads(stamp.read_text()) != actual:
        raise RuntimeError(f"Binaire ou sources modifiés pour {phase}/{arch}. Recompiler avec --preparer.")
    return actual


def measure(binary, sector, cache, rep, specific):
    args = ["/usr/bin/time", "-l", binary, str(sector), cache, str(rep)]
    if specific:
        args.append(specific)
    start = datetime.now(timezone.utc)
    proc = subprocess.run(args, capture_output=True, text=True)
    if proc.returncode:
        raise RuntimeError(proc.stderr + '\n' + proc.stdout[-250:])
    memory = re.search(r"^memory (\d+) (\d+)$", proc.stderr, re.M)
    rss = re.search(r"(\d+)\s+maximum resident set size", proc.stderr)
    if not memory or not rss:
        raise RuntimeError("rapport mémoire Darwin incomplet")
    rows = list(csv.DictReader(proc.stdout.splitlines(), fieldnames=HEADER))
    assert rows and all(None not in row and None not in row.values() for row in rows)
    return rows, dict(tas_pic=int(memory[1]), pile_travail=int(memory[2]), rss_pic=int(rss[1]),
                      debut_utc=start.isoformat(), fin_utc=datetime.now(timezone.utc).isoformat())


def percentile(samples, q):
    return sorted(samples)[math.ceil(len(samples)*q)-1]


def summary(rows):
    n = len(rows)
    sums = {col: sum(int(row[col]) for row in rows) for col in HEADER[6:]}
    times = [int(r["mur_ns"]) for r in rows]
    reads = sum(sums[k] for k in HEADER if k.startswith("lectures_"))
    writes = sum(sums[k] for k in HEADER if k.startswith("ecritures_"))
    size = int(rows[0]["taille"])
    return dict(n=n, p50_ns=percentile(times,.50), p95_ns=percentile(times,.95),
                p99_ns=percentile(times,.99), max_ns=max(times),
                debit_mio_s=n*size*1e9/sums["mur_ns"]/2**20,
                amplification=writes*4096/(size*n), lectures_par_op=reads/n,
                ecritures_par_op=writes/n, **sums)


def save_summary(directory, raw):
    groups = defaultdict(list)
    with gzip.open(raw, "rt", newline="") as f:
        for row in csv.DictReader(f):
            key = tuple(row[k] for k in KEY)
            groups[("toutes", *key)].append(row)
            groups[(row["campagne"], *key)].append(row)
    dest = directory / "distributions.csv.gz"
    trajectories = {}
    for (rep, arch, phase, sector, cache, topology, order, size, operation), rows in groups.items():
        if rep == "toutes":
            continue
        # Le tirage et la taille ne dépendent ni de l'ISA, ni du format, ni
        # du secteur du périphérique. Une paire incomplète fausserait les ratios.
        key = rep, topology, order, size, operation
        positions = tuple((row["indice"], row["offset"]) for row in rows)
        if positions != trajectories.setdefault(key, positions):
            raise RuntimeError(f"trajectoires non appariées {key}/{arch}/{phase}/{sector}/{cache}")
        expected = 32 if operation == "lecture" else 8
        if len(rows) != expected:
            raise RuntimeError(f"cellule incomplète {key}")
    with gzip.open(dest, "wt", newline="") as f:
        writer = None
        by_run = {}
        for key, rows in sorted(groups.items()):
            data = dict(zip(["campagne", *KEY], key))
            data.update(summary(rows))
            if key[0] != "toutes":
                by_run[key] = data
            if writer is None:
                writer = csv.DictWriter(f, fieldnames=data.keys())
                writer.writeheader()
            writer.writerow(data)

    # Chaque ratio compare le même jeu d’offsets et le même indice de campagne.
    with gzip.open(directory / "comparaisons.csv.gz", "wt", newline="") as f:
        writer = None
        for key in sorted(k for k in groups if k[0] == "toutes" and k[2] == "P5.1"):
            _, arch, _, *charge = key
            for reference in ("P4.1", "P5"):
                if reference == "P4.1" and charge[2] != "origine":
                    continue
                pairs = []
                for run_key, current in by_run.items():
                    if run_key[1:] == key[1:]:
                        old = by_run.get((run_key[0], arch, reference, *charge))
                        if old:
                            pairs.append((current, old))
                if not pairs:
                    continue
                data = dict(zip(KEY, key[1:]))
                data.update(reference=reference, campagnes=len(pairs))
                for name in ("debit_mio_s", "p95_ns", "p99_ns", "cpu_ns", "allocations",
                             "lectures_par_op", "amplification"):
                    ratios = [c[name] / o[name] for c, o in pairs if o[name]]
                    for label, fn in [("min", min), ("mediane", statistics.median), ("max", max)]:
                        data[f"{name}_{label}"] = fn(ratios) if ratios else ""
                if writer is None:
                    writer = csv.DictWriter(f, fieldnames=data.keys())
                    writer.writeheader()
                writer.writerow(data)


def schedule(args, archs, phases):
    for rep in range(args.campagnes):
        for arch_index, arch in enumerate(archs):
            sectors = args.secteur or [512, 4096]
            for si, sector in enumerate(sectors if rep % 2 == 0 else sectors[::-1]):
                caches = args.cache or CONDITIONS
                rotation = rep % len(caches)
                for ci, cache in enumerate(caches[rotation:] + caches[:rotation]):
                    start = (rep + arch_index + si + ci) % len(phases)
                    for phase in phases[start:] + phases[:start]:
                        yield rep, arch, sector, cache, phase


def run_trial(work, binary, digest, sector, cache, rep, args):
    log = work / f"essai-{digest}-{sector}-{cache}-{rep}-{args.topologie or 'toutes'}.json.gz"
    if args.reprendre and log.exists():
        with gzip.open(log, "rt") as f:
            rows, memory = json.load(f)
        memory.setdefault("fin_utc", datetime.fromtimestamp(log.stat().st_mtime, timezone.utc).isoformat())
    else:
        rows, memory = measure(binary, sector, cache, rep, args.topologie)
        if args.reprendre:
            with gzip.open(log.with_suffix(".tmp"), "wt") as f:
                json.dump([rows, memory], f)
            log.with_suffix(".tmp").replace(log)
    return rows, memory


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--sortie", required=True, type=Path)
    ap.add_argument("--travail", type=Path, help="Répertoire temporaire de compilation réutilisable")
    ap.add_argument("--phase", choices=PHASES, action="append")
    ap.add_argument("--arch", choices=CPUS, action="append")
    ap.add_argument("--secteur", choices=[512, 4096], type=int, action="append")
    ap.add_argument("--cache", choices=CONDITIONS, action="append")
    ap.add_argument("--campagnes", default=2, type=int)
    ap.add_argument("--reprendre", action="store_true",
                    help="Réutiliser les essais terminés par le même binaire dans --travail")
    ap.add_argument("--preparer", action="store_true", help="Compiler seulement")
    ap.add_argument("--sans-compilation", action="store_true")
    ap.add_argument("--topologie", choices=["origine", "snapshot", "trois", "clone"])
    args = ap.parse_args()
    if args.campagnes < 1:
        ap.error("au moins une campagne")
    if not args.travail and (args.preparer or args.sans_compilation or args.reprendre):
        ap.error("ces modes nécessitent --travail")
    archs = args.arch or list(CPUS)
    phases = args.phase or list(PHASES)
    if args.topologie and args.topologie != "origine" and "P4.1" in phases:
        ap.error("La topologie demandée requiert le partage P5")
    args.sortie.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="novafs-matrice-") as tmp:
        work = args.travail or Path(tmp)
        work.mkdir(parents=True, exist_ok=True)
        binaries = {}
        provenance = {}
        for arch in archs:
            for phase in phases:
                if args.sans_compilation:
                    path = work / f"matrice-{arch}-{phase}"
                    if not path.is_file():
                        raise RuntimeError(f"binaire absent {path}")
                else:
                    path = build(work, arch, phase)
                provenance[f"{arch}-{phase}"] = verify_binary(work, arch, phase, path)
                binaries[arch, phase] = path
        if args.preparer:
            return
        meta = dict(
            macOS=platform.mac_ver()[0], cpu=command("sysctl", "-n", "machdep.cpu.brand_string"),
            rust=command("rustup", "run", "stable", "rustc", "-V"), historique=PHASES,
            sources_banc={name: fingerprint(LAB / name) for name in
                          ["p51_matrice.rs", "heap_meter.rs", "thread_clock.rs", "p51-matrice.py"]},
            provenance=provenance,
            binaires={f"{a}-{p}": fingerprint(path) for (a, p), path in binaries.items()},
            horloges={a: json.loads(command(binaries[a, phases[0]], "horloge")) for a in archs},
            repetitions=args.campagnes, executions=[])
        raw = args.sortie / "echantillons.csv.gz"
        with gzip.open(raw, "wt", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=["campagne", "arch", "phase", "secteur", "cache", *HEADER])
            writer.writeheader()
            for rep, arch, sector, cache, phase in schedule(args, archs, phases):
                label = dict(campagne=rep+1, arch=arch, phase=phase, secteur=sector, cache=cache)
                print(label, flush=True)
                digest = meta["binaires"][f"{arch}-{phase}"]
                rows, memory = run_trial(work, binaries[arch, phase], digest, sector, cache, rep, args)
                memory["tas_et_pile_sous_7_Mio"] = memory["tas_pic"] + memory["pile_travail"] < 7 * 2**20
                meta["executions"].append({**label, **memory, "n": len(rows)})
                for row in rows:
                    writer.writerow({**label, **row})
                stream.flush()
        save_summary(args.sortie, raw)
        (args.sortie / "campagnes.json").write_text(json.dumps(meta, ensure_ascii=False, indent=2) + "\n")
        if any(not e["tas_et_pile_sous_7_Mio"] for e in meta["executions"]):
            raise SystemExit("Budget mémoire dépassé ; les mesures sont conservées.")


if __name__ == "__main__":
    main()
