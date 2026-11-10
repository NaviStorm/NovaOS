#!/usr/bin/env python3
"""Rejouer le contrôle P0 à P5.1 et extraire les oracles du corpus P6."""
import argparse
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys

LAB = Path(__file__).resolve().parent
SUITES = runpy.run_path(str(LAB / "p51-validation-suites.py"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("architecture", choices=("aarch64", "x86_64"))
    parser.add_argument("--sortie", type=Path, required=True)
    args = parser.parse_args()
    env = dict(os.environ, NOVAFS_TARGET=f"{args.architecture}-apple-darwin", CARGO_NET_OFFLINE="true")
    historique = json.loads(subprocess.check_output([sys.executable, LAB / "p51-validation-suites.py", args.architecture], text=True, env=env))
    extrait = SUITES["cargo"](env, "--test", "p6_compression", "--test", "p6_restitution", "--", "--nocapture", "--test-threads=1")
    report = {"historique_et_suites":historique, "p6_cible":SUITES["totaux"](extrait)}
    for marque, n in [("P6_ALEA", 4), ("P6_PANNE", 2), ("P6_MEMOIRE", 4)]:
        lignes = SUITES["mesures"](extrait, marque)
        if len(lignes) != n:
            raise ValueError(f"corpus incomplet {marque}: {lignes}")
        report[marque] = lignes
    for ligne in report['P6_MEMOIRE']:
        avant, apres = map(int, re.search(r'avant=(\d+) apres=(\d+)', ligne).groups())
        assert avant == apres
    args.sortie.parent.mkdir(parents=True, exist_ok=True)
    args.sortie.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
