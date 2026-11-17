"""Rejouer les cibles de régression du monorepo, une réservation par cible."""

import argparse
import fcntl
from pathlib import Path
import os
import subprocess
import sys

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("sortie", type=Path)
options = p.parse_args()
out = options.sortie.resolve()
out.mkdir(parents=True, exist_ok=False)
root = Path(__file__).resolve().parents[1]
targets = (
    "test",
    "elf-test",
    "lifecycle-test",
    "ipc-races",
    "block-test",
    "cli-test",
    "cli-p9-test",
    "nsh-test",
    "nsh-scripts-test",
    "nsh-terminal-test",
    "vi-test",
)
failures = []
with (out / "resultats.tsv").open("w", buffering=1) as table:
    for arch in ("aarch64", "x86_64"):
        for target in targets:
            with (Path(os.environ["NOVAOS_TMPDIR"]) / "arc027-mesures.lock").open(
                "a"
            ) as lock:
                fcntl.flock(lock, fcntl.LOCK_EX)
                with (out / f"{arch}-{target}.log").open("w") as f:
                    cmd = ["make", "-C", str(root / "novaOS"), "ARCH=" + arch, target]
                    r = subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT)
            print(arch, target, r.returncode, sep="\t", file=table)
            if r.returncode:
                failures.append([arch, target])
if failures:
    sys.exit(str(failures))
