"""Exécute le banc sleep sur macOS ARM64 ou sur volumes QEMU P9 jetables."""

import argparse
import os
from pathlib import Path
import subprocess as sp
import sys
from host import HERE

action = sys.argv[1]
scripts = {"options": "tester-options-sleep.sh", "mesures": "mesurer-performances-sleep.sh"}
p = argparse.ArgumentParser(prog=scripts[action], description=__doc__)
p.add_argument("--cible", choices=["macos", "novaos"], default="macos")
p.add_argument(
    "--arch",
    choices=["aarch64", "x86_64"],
    help="limite les essais NovaOS à une architecture",
)
g = p.add_mutually_exclusive_group()
g.add_argument("--rapide", action="store_true", help="sous-ensemble court (défaut)")
g.add_argument("--complet", action="store_true", help="matrice complète")
p.add_argument(
    "--out", type=Path, help="fichier JSON sur macOS, dossier des résultats sur NovaOS"
)
a = p.parse_args(sys.argv[2:])
if a.cible == "macos":
    if a.arch:
        p.error("--arch s’applique à NovaOS")
    cmd = [
        sys.executable,
        HERE / "lab/host.py",
        action,
        "--complet" if a.complet else "--rapide",
    ]
    if a.out:
        cmd.extend(["--out", a.out])
    sp.run(cmd, check=True)
else:
    root = HERE.parent
    out = a.out or Path(os.environ["TMPDIR"]) / "sleep-rejeu" / action
    sp.run(
        [root / "novaFS/lab/run", "build", "--offline", "--bin", "novafs"],
        cwd=root,
        check=True,
        env=dict(os.environ, NOVAFS_TARGET="aarch64-apple-darwin"),
    )
    for arch in [a.arch] if a.arch else ["aarch64", "x86_64"]:
        if action == "options":
            sp.run(
                ["make", "-C", root / "novaOS", "ARCH=" + arch, "kernel"], check=True
            )
        for profile in ["sdk", "nsh"] if action == "options" else ["sdk"]:
            cmd = [sys.executable, HERE / "lab/invite.py", arch, profile, "--out", out]
            if not a.complet:
                cmd.append("--rapide")
            if action == "mesures":
                cmd.append("--mesures")
            sp.run(cmd, check=True)
