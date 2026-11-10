#!/usr/bin/env python3
"""Coupe un vrai processus ou une VM possédant uniquement des images jetables."""
import argparse
import json
import os
from pathlib import Path
import selectors
import shutil
import signal
import subprocess
import sys
import tempfile

LAB = Path(__file__).resolve().parent
BLOCK = 4096
BLOCKS = 320


def executable():
    lines = subprocess.check_output(
        [LAB / "run", "build", "--offline", "--example", "p1-image", "--message-format=json"],
        text=True,
    )
    return next(record["executable"] for record in map(json.loads, lines.splitlines())
                if record.get("executable") and record["target"]["name"] == "p1-image")


def passes(binary, action, image, sector, *args):
    with image.open("rb" if action == "check" else "r+b") as fd:
        return subprocess.check_output([binary, action, str(sector), *args], stdin=fd,
                                       text=True, timeout=60).strip()


def operations(journal):
    ops = []
    for command in journal.splitlines():
        if command == "F":
            ops.append((None, b""))
        else:
            op, index, payload = command.split()
            block, data = int(index), bytes.fromhex(payload)
            if op != "W" or not 0 <= block < BLOCKS or len(data) != BLOCK:
                raise ValueError("écriture invalide dans le journal")
            ops.append((block, data))
    barriers = [i + 1 for i, (block, _) in enumerate(ops) if block is None]
    # Avant F1, uniquement des dépendances. Entre F1 et F2, l'ancre de G6.
    if (not 4 <= len(ops) <= 67 or barriers != [len(ops) - 2, len(ops)]
            or any(b < 2 for b, _ in ops[:barriers[0] - 1]) or ops[-2][0] != 0):
        raise ValueError("ordre de publication inattendu")
    return ops, barriers


def cut_writer(binary, image, sector, step):
    with image.open("r+b") as fd, subprocess.Popen(
        [binary, "cut", str(sector), str(step)], stdin=fd, stdout=subprocess.PIPE,
    ) as child:
        try:
            with selectors.DefaultSelector() as ready:
                ready.register(child.stdout, selectors.EVENT_READ)
                if not ready.select(timeout=30):
                    raise TimeoutError("le moteur n’a pas signalé la frontière demandée")
                if child.stdout.readline() != f"HALTE {step}\n".encode():
                    raise RuntimeError("marque de coupure absente")
        finally:
            child.kill()
            child.wait(timeout=10)
        if child.returncode != -signal.SIGKILL:
            raise RuntimeError(f"arrêt inattendu du moteur : {child.returncode}")


def campaign(binary, directory, sector, supports):
    seed = directory / "base.raw"
    work = directory / "travail.raw"
    with seed.open("wb") as fd:
        fd.truncate(BLOCKS * BLOCK)
    passes(binary, "seed", seed, sector)
    passes(binary, "check", seed, sector, "ancien")
    shutil.copyfile(seed, work)
    ops, (f1, f2) = operations(passes(binary, "journal", work, sector))
    passes(binary, "check", work, sector, "nouveau")
    for support in supports:
        steps = (range(f2 + 2) if support == "fichier" else [0, f1 - 1, f1, f1 + 1, f2])
        for step in steps:
            shutil.copyfile(seed, work)
            counts = ""
            if support == "fichier":
                cut_writer(binary, work, sector, step)
            else:
                from p1_qemu import cut_vm
                observed = cut_vm(work, sector, ops, step)
                counts = f" écritures_QMP={observed['writes']} flushs_QMP={observed['flushes']}"
            expected = "ancien" if step <= f1 else "nouveau"
            if support == "qemu" and f1 < step < f2:
                expected = "incertain"
            first = passes(binary, "check", work, sector, expected)
            second = passes(binary, "check", work, sector, expected)
            if first != second:
                raise RuntimeError("la tête diffère entre les deux remontages en lecture seule")
            print(f"{support} secteur={sector} étape={step} F1={f1} F2={f2} "
                  f"attendu={expected} récupéré={first} remontages=2{counts}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--support", choices=["fichier", "qemu", "tous"], default="fichier")
    options = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("le banc de validation exige macOS")
    binary = executable()
    print(f"binaire={binary} cible={os.environ.get('NOVAFS_TARGET', 'native')}", flush=True)
    supports = ["fichier", "qemu"] if options.support == "tous" else [options.support]
    with tempfile.TemporaryDirectory(prefix="novafs-p1-") as name:
        for sector in (512, 4096):
            campaign(binary, Path(name), sector, supports)


if __name__ == "__main__":
    main()
