#!/usr/bin/env python3
"""Qualifie les formats historiques sur macOS et les deux exécutables Darwin."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

LAB = Path(__file__).resolve().parent
BASE = "3ca74ee"
HIST = runpy.run_path(str(LAB / "p51-validation.py"))


def lancer(nom, *args, arch=None):
    env = dict(os.environ, CARGO_NET_OFFLINE="true")
    if arch:
        env["NOVAFS_TARGET"] = arch + "-apple-darwin"
    return subprocess.check_output([sys.executable, LAB / nom, *args], text=True, env=env)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--archives", type=Path, help="sources déjà extraites par git archive, par commit")
    ap.add_argument("--support", choices=["formats", "pannes"], required=True)
    arg = ap.parse_args()
    if arg.support == "pannes":
        result = {}
        for arch in ("aarch64", "x86_64"):
            rows = lancer("test-persistence.py", "--support", "tous", arch=arch).splitlines()[1:]
            for medium, expected in (("fichier ", 24), ("qemu ", 10)):
                assert sum(r.startswith(medium) and "remontages=2" in r for r in rows) == expected
            result[arch] = rows
    else:
        with tempfile.TemporaryDirectory(prefix="novafs-p6-historique-") as name:
            temp = Path(name)
            anciens = HIST["historiques"](temp, arg.archives)
            source = (arg.archives / BASE / "novaFS/lab/run" if arg.archives
                      else HIST["extraire"](BASE, temp)).resolve()
            anciens[BASE] = {s: lancer(s, "--reference", source).splitlines()
                             for s in ("test-interop.py", "test-pool-interop.py")}
            partage = HIST["campagne"](source, temp)
            for essai in partage:
                for key in ("producteur", "lecteur"):
                    essai[key] = BASE if essai[key] == "P5" else "v0.9.8-format-P5"
            result = dict(images=anciens, partage=partage,
                          pools=json.loads(lancer("p5-echange.py")),
                          exports=json.loads(lancer("p5-export.py")))
            subprocess.run([sys.executable, LAB / "test-latency-report.py"], check=True)
            result["test_rapport_latences"] = "réussi"
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
