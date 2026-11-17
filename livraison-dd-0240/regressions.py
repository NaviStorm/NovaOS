"""Rejouer les cibles de régression du monorepo, une réservation par architecture."""

import argparse
import fcntl
from pathlib import Path
import os
import subprocess
import sys

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("sortie", type=Path)
p.add_argument(
    "--reprendre",
    action="store_true",
    help="continuer un relevé interrompu dans le même checkout",
)
options = p.parse_args()
out = options.sortie.resolve()
out.mkdir(parents=True, exist_ok=options.reprendre)
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
results = out / "resultats.tsv"
done = {}
if options.reprendre and results.exists():
    for row in results.read_text().splitlines():
        arch, target, rc = row.split("\t")
        if not (out / f"{arch}-{target}.log").is_file():
            sys.exit(f"Journal absent pour {arch} {target}")
        done[arch, target] = int(rc)

with results.open("a", buffering=1) as table:
    for arch in ("aarch64", "x86_64"):
        pending = [target for target in targets if (arch, target) not in done]
        if not pending:
            continue
        with (Path(os.environ["NOVAOS_TMPDIR"]) / "arc027-mesures.lock").open(
            "a"
        ) as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            for target in pending:
                log = out / f"{arch}-{target}.log"
                with log.open("a") as f:
                    cmd = ["make", "-C", str(root / "novaOS"), "ARCH=" + arch, target]
                    r = subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT)
                print(arch, target, r.returncode, sep="\t", file=table)
                done[arch, target] = r.returncode
failures = [key for key, code in done.items() if code]
if failures:
    sys.exit(str(failures))
