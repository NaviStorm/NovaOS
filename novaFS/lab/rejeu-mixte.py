#!/usr/bin/env python3
"""Compare les cinq séries mixtes P4/P4.1 sur un Mac, sans les petits fichiers."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import struct
import subprocess
import sys
import tempfile
import time

LAB = Path(__file__).resolve().parent
ROOT = LAB.parent.parent
MEASURES = runpy.run_path(str(LAB / "measure-performance.py"))
ARCHS = MEASURES["TARGETS"]
PHASES = ("ecriture_seq64k", "lecture_seq64k", "lecture_aleatoire4k", "ecriture_aleatoire4k")
P4 = "0a89d23"


def once(source, old, new):
    if source.count(old) != 1:
        raise ValueError(f"repère du banc historique absent ou ambigu : {old!r}")
    return source.replace(old, new, 1)


def harness(stamp=False):
    source = (LAB / "novafs-bench.rs").read_text()
    # La boucle existante et la vérification des lectures restent intactes.
    # Après la cinquième série, on vérifie l'image et on la démonte.
    start = '    e.mkdir("/petits")?;'
    end = '\nfn main() -> Result<()> {'
    if source.count(start) != 1 or source.count(end) != 1:
        raise ValueError("bornes de la charge mixte inconnues")
    before, tail = source.split(start)
    _, after = tail.split(end)
    source = before + '    e.verify()?;\n    drop(e.unmount()?);\n    Ok(())\n}\n' + end + after
    if stamp:
        # Datation hors des chronos d'opération et de boucle, identique pour les deux moteurs.
        start = '    for series in 1..=5 {\n        measure(\n            "ecriture_seq64k",'
        clock = "std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_micros()"
        source = once(source, start, start.replace("        measure(", f"        let debut_us = {clock};\n        measure("))
        end = "    }\n    e.verify()?;\n    drop(e.unmount()?);\n    Ok(())\n}\n\nfn main()"
        source = once(source, end, '        println!("horodatage_serie\\t{series}\\t{debut_us}\\t{}", '
                      + clock + ");\n" + end)
    # L'export des durées ne se produit qu'après l'arrêt des chronos du lot.
    return once(source, 'if label == "lecture_seq64k" {',
                'if matches!(label, "lecture_seq64k" | "ecriture_seq64k" | '
                '"lecture_aleatoire4k" | "ecriture_aleatoire4k") {')


def packages(tmp, bench):
    current = tmp / "P4.1" / "novaFS"
    shutil.copytree(LAB.parent, current, ignore=shutil.ignore_patterns("target", "docs", "__pycache__", ".git"))
    if 'version = "0.9.5"' not in (current / "Cargo.toml").read_text():
        raise ValueError("exécuter depuis la révision NovaFS P4.1 v0.9.5")
    archive = tmp / "P4"
    archive.mkdir()
    with subprocess.Popen(["git", "archive", P4, "novaFS"], cwd=ROOT, stdout=subprocess.PIPE) as git:
        subprocess.run(["tar", "-xf", "-", "-C", archive], stdin=git.stdout, check=True)
        if git.wait():
            raise RuntimeError("extraction P4 échouée")
    old = archive / "novaFS"
    manifest = old / "Cargo.toml"
    manifest.write_text(once(manifest.read_text(), "[features]", "[features]\nlatency-probes = []"))
    for pkg in (old, current):
        (pkg / "lab/novafs-bench.rs").write_text(bench)
        for file in ("heap_meter.rs", "performance.rs"):
            shutil.copyfile(LAB / file, pkg / "lab" / file)
    return {"P4": old, "P4.1": current}


def build(pkg, arch):
    target = arch + "-apple-darwin"
    subprocess.run([pkg / "lab/run", "build", "--offline", "--bin", "novafs-bench", "--features", "bench-coordinator"],
                   env={**os.environ, "NOVAFS_TARGET": target, "CARGO_TARGET_DIR": str(pkg / "target")}, check=True)
    exe = pkg / "target" / target / "release/novafs-bench"
    with exe.open("rb") as binary:
        if binary.read(8) != bytes.fromhex("cffaedfe") + struct.pack("<I", ARCHS[arch]):
            raise ValueError(f"binaire Mach-O {arch} attendu")
    return exe


def summarize(lines, phase):
    samples, batches, instrumentation = {}, {}, {}
    for line in lines:
        cols = line.split("\t")
        if cols[:2] == ["echantillons_ns", phase]:
            s = int(cols[2])
            if s in samples:
                raise ValueError("échantillons dupliqués")
            samples[s] = list(map(int, cols[3:]))
        elif cols[0] == phase:
            batches[int(cols[1])] = list(map(int, cols[2:]))
        elif cols[:2] == ["instrumentation", phase]:
            instrumentation[int(cols[2])] = list(map(int, cols[3:]))
    keys = set(range(1, 6))
    if not samples.keys() == batches.keys() == instrumentation.keys() == keys:
        raise ValueError("séries manquantes")
    for s, row in batches.items():
        dist = MEASURES["distribution"](samples[s])
        if [dist[key] for key in ("n", "p50_ns", "p95_ns", "p99_ns", "max_ns")] != [row[0], *row[3:7]]:
            raise ValueError("distribution incohérente")
    elapsed = sum(row[2] for row in batches.values())
    f1, f2, time_f1, time_f2, allocations = (sum(row[i] for row in instrumentation.values()) for i in range(5))
    n = sum(len(a) for a in samples.values())
    rest = elapsed - time_f1 - time_f2
    if rest < 0:
        raise ValueError("temps des barrières supérieur à la durée de la phase")
    return {
        **MEASURES["distribution"](x for group in samples.values() for x in group),
        "debit_mio_s": sum(row[1] for row in batches.values()) * 1e9 / (2**20 * elapsed),
        "duree_ns": elapsed, "f1": f1, "f2": f2, "f1_ns": time_f1, "f2_ns": time_f2,
        "hors_barrieres_moyen_ns_par_operation": rest / n, "allocations": allocations,
        "blocs_lus": sum(row[7] for row in batches.values()), "blocs_ecrits": sum(row[8] for row in batches.values()),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sortie", required=True, type=Path, help="nouveau fichier JSON du rejeu")
    parser.add_argument("--arch", choices=ARCHS, help="une seule architecture (par défaut : les deux)")
    parser.add_argument("--inverse", action="store_true", help="inverser l’ordre P4/P4.1 dans chaque architecture")
    parser.add_argument("--horodater", action="store_true", help="inscrire le début et la fin UTC de chaque série")
    args = parser.parse_args()
    if sys.platform != "darwin" or args.sortie.exists():
        parser.error("macOS et un nouveau fichier de résultats sont requis")
    bench = harness(args.horodater)
    arches = [args.arch] if args.arch else list(ARCHS)
    report = {
        "reference": P4, "moteur_courant": "0.9.5", "cache_hote": "ordinaire",
        "harnais_sha256": hashlib.sha256(bench.encode()).hexdigest(), "series_par_phase": 5, "passages": [],
    }
    with tempfile.TemporaryDirectory(prefix="novafs-rejeu-mixte-") as work:
        sources = packages(Path(work), bench)
        # Toutes les compilations s'achèvent avant la première mesure.
        binaries = {(v, a): build(src, a) for v, src in sources.items() for a in arches}
        order = []
        for arch in arches:
            versions = ["P4", "P4.1"] if arch == "aarch64" else ["P4.1", "P4"]
            order.extend((arch, version) for version in (reversed(versions) if args.inverse else versions))
        for arch, version in order:
            exe = binaries[version, arch]
            sector = 4096 if arch == "aarch64" else 512
            print(f"mesure {arch} {version}", flush=True)
            start, load = time.time(), os.getloadavg()
            item = MEASURES["measure"](exe, sector, "complet")
            item.update(architecture=arch, moteur=version, sha256_binaire=hashlib.sha256(exe.read_bytes()).hexdigest(),
                        debut_epoch_s=start, fin_epoch_s=time.time(), charge_hote_avant=load,
                        charge_hote_apres=os.getloadavg(), scenario="cinq_series_mixtes")
            item["phases"] = {phase: summarize(item["journal"], phase) for phase in PHASES}
            if args.horodater:
                rows = [list(map(int, line.split("\t")[1:])) for line in item["journal"]
                        if line.startswith("horodatage_serie\t")]
                if [r[0] for r in rows] != list(range(1, 6)) or any(len(r) != 3 or r[1] >= r[2] for r in rows):
                    raise ValueError("horodatages des séries manquants ou incohérents")
                item["horodatages"] = [dict(serie=s, debut_epoch_us=a, fin_epoch_us=b) for s, a, b in rows]
            report["passages"].append(item)
    with args.sortie.open("x") as out:
        json.dump(report, out, ensure_ascii=False, separators=(",", ":"))
        out.write("\n")


if __name__ == "__main__":
    main()
