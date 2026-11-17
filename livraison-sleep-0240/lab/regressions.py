"""Régressions sur deux architectures, avec une réservation de mesure par architecture."""

import argparse
import fcntl
import os
import subprocess
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--out", type=Path, required=True)
p.add_argument(
    "--reprendre",
    action="store_true",
    help="garder les cibles déjà réussies sur ces sources",
)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
targets = (
    "test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test "
    "nsh-test nsh-scripts-test nsh-terminal-test vi-test"
).split()
table = a.out / "regressions.tsv"
passed = set()
if a.reprendre and table.exists():
    for line in table.read_text().splitlines():
        arch, target, code = line.split("\t")
        if code == "0":
            passed.add((arch, target))
failed = []
with table.open("a" if a.reprendre else "w", buffering=1) as summary:
    for arch in ("aarch64", "x86_64"):
        with (Path(os.environ["NOVAOS_TMPDIR"]) / "arc027-mesures.lock").open(
            "a+"
        ) as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            for target in targets:
                if (arch, target) in passed:
                    continue
                with (a.out / f"{arch}-{target}.log").open("w") as log:
                    result = subprocess.run(
                        ["make", "-C", root / "novaOS", "ARCH=" + arch, target],
                        stdout=log,
                        stderr=subprocess.STDOUT,
                    )
                print(arch, target, result.returncode, sep="\t", file=summary)
                if result.returncode:
                    failed.append((arch, target))
if failed:
    raise SystemExit(f"Échecs: {failed}")
