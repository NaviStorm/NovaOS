#!/usr/bin/env python3
"""Rejouer le client mixte P5 dans deux ordres de passage avec un binaire par phase et architecture."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import platform
import subprocess
import tarfile
import tempfile

LAB = Path(__file__).resolve().parent
REPO = LAB.parent.parent
PHASES = ("P4.1", "P5", "P5.1")
ARCHS = ("aarch64", "x86_64")
SECTEURS = (512, 4096)
historique = importlib.util.spec_from_file_location("mesures_p5", LAB / "p5-mesures.py")
p5 = importlib.util.module_from_spec(historique)
historique.loader.exec_module(p5)


def git(*args):
    return subprocess.check_output(["git", "-C", REPO, *args])


def identite_source(root):
    files = sorted((root / "src").rglob("*.rs")) + [root / "Cargo.toml", root / "Cargo.lock"]
    digest = hashlib.sha256()
    for file in files:
        digest.update(file.relative_to(root).as_posix().encode() + b"\0")
        digest.update(hashlib.sha256(file.read_bytes()).digest())
    return digest.hexdigest()


def compiler(work, ref_final):
    refs = {"P4.1": "b865c58", "P5": "ae508ce", "P5.1": ref_final}
    sources, binaries = {}, {}
    for phase, ref in refs.items():
        base = work / phase
        base.mkdir()
        root = REPO
        if ref != "travail":
            root = base / ("b865c58" if phase == "P4.1" else "revision")
            root.mkdir()
            with tarfile.open(fileobj=io.BytesIO(git("archive", ref, "novaFS"))) as archive:
                archive.extractall(root, filter="data")
        commit = git("rev-parse", "HEAD" if ref == "travail" else ref).decode().strip()
        sources[phase] = {"ref": ref, "commit_de_base": commit,
                          "sha256_sources_moteur": identite_source(root / "novaFS")}
        # Le constructeur historique prend le moteur partagé dans REPO pour P5.
        # P4.1 est lu dans base/b865c58, déjà extrait et identifié ci-dessus.
        p5.REPO = root
        for arch in ARCHS:
            binaries[phase, arch] = p5.build(arch, "P4.1" if phase == "P4.1" else "P5", base)
    return sources, binaries


def ordres(repetition):
    geoms = [(arch, size) for arch in ARCHS for size in SECTEURS]
    if repetition == 2:
        geoms.reverse()
    for arch, size in geoms:
        debut = (2 * ARCHS.index(arch) + SECTEURS.index(size)) % len(PHASES)
        phases = PHASES[debut:] + PHASES[:debut]
        # À géométrie identique, les positions relatives de chaque paire
        # s'inversent au second passage. L'ordre initial tourne entre géométries.
        yield arch, size, phases if repetition == 1 else phases[::-1]


def ecrire(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


def utc():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def synthese(sortie):
    lignes = []
    for i in (1, 2):
        for arch in ARCHS:
            for secteur in SECTEURS:
                operations = {phase: json.loads((sortie / f"r{i}-{arch}-{phase}-{secteur}.json")
                                               .read_text())["operations"] for phase in PHASES}
                for op in ("read", "write"):
                    cout = {}
                    for phase, stats in operations.items():
                        sample = stats[op]
                        n = sample["n"]
                        if n != 128 or sample["synchronisations"] != (2*n if op == "write" else 0):
                            raise ValueError(f"échantillons ou barrières incomplets pour {phase}/{op}")
                        cout[phase] = {
                            "debit_mio_s": sample["debit_mio_s"],
                            "mur_moy_us": sample["total_ns"] / n / 1e3,
                            "flush_moy_us": sample["temps_sync_ns"] / n / 1e3,
                            "hors_flush_moy_us": (sample["total_ns"] - sample["temps_sync_ns"]) / n / 1e3,
                            "p95_us": sample["p95_ns"] / 1e3, "p99_us": sample["p99_ns"] / 1e3,
                        }
                    base, final = cout["P5"], cout["P5.1"]
                    lignes.append({"repetition": i, "arch": arch, "secteur": secteur, "operation": op,
                                   "par_phase": cout,
                                   "ratio_debit_final_sur_p5": final["debit_mio_s"] / base["debit_mio_s"],
                                   "ratio_debit_final_sur_p41": final["debit_mio_s"] / cout["P4.1"]["debit_mio_s"],
                                   "delta_mur_moy_us": final["mur_moy_us"] - base["mur_moy_us"],
                                   "delta_flush_moy_us": final["flush_moy_us"] - base["flush_moy_us"],
                                   "delta_hors_flush_moy_us": final["hors_flush_moy_us"] - base["hors_flush_moy_us"]})
    ecrire(sortie / "synthese.json", lignes)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--sortie", type=Path, required=True)
    ap.add_argument("--p51", default="travail", help="ref Git pour P5.1, ou travail")
    args = ap.parse_args()
    for name in ("p5-mesures.py", "p5_mesures.rs", "heap_meter.rs"):
        if (LAB / name).read_bytes() != git("show", f"ae508ce:novaFS/lab/{name}"):
            raise ValueError(f"fichier du banc historique différent de P5 : {name}")
    args.sortie.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="novafs-mixte-apparie-") as tmp:
        sources, binaries = compiler(Path(tmp), args.p51)
        provenance = dict(
            date_debut=utc(), plateforme=platform.platform(), machine=platform.machine(),
            macos=platform.mac_ver()[0], rustc=subprocess.check_output(["rustup", "run", "stable", "rustc", "-Vv"],
                                                                       text=True).strip(),
            sources=sources, client_sha256=hashlib.sha256((LAB/"p5_mesures.rs").read_bytes()).hexdigest(),
            lanceur_historique_sha256=hashlib.sha256((LAB/"p5-mesures.py").read_bytes()).hexdigest(),
            executables_sha256={f"{phase}-{arch}": hashlib.sha256(b.read_bytes()).hexdigest()
                                 for (phase, arch), b in binaries.items()},
            commande=f"python3 novaFS/lab/p51-mixte.py --p51 {args.p51} --sortie <dossier>",
            cache="politique historique de FileImage et macOS ; chaque processus crée une image temporaire",
            passages=[])
        for i in (1, 2):
            for arch, secteur, phases in ordres(i):
                for phase in phases:
                    nom = f"r{i}-{arch}-{phase}-{secteur}"
                    print(nom, flush=True)
                    debut = utc()
                    p5.measure(binaries[phase, arch], secteur, args.sortie / nom)
                    provenance["passages"].append(dict(repetition=i, architecture=arch, secteur=secteur,
                                                       phase=phase, fichier=nom, debut_utc=debut, fin_utc=utc()))
                    ecrire(args.sortie / "provenance.json", provenance)
        synthese(args.sortie)


if __name__ == "__main__":
    main()
