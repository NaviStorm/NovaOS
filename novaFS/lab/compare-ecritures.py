#!/usr/bin/env python3
"""Rejoue le journal P1 sur un fichier macOS, avec les deux barrières effectives."""
import fcntl
import json
import os
from pathlib import Path
import platform
import runpy
import shutil
import tempfile
from time import perf_counter_ns

p1 = runpy.run_path(str(Path(__file__).with_name("test-persistence.py")))


def commands(ops, grouped):
    result = []
    for block, payload in ops:
        if (grouped and block is not None and result and result[-1][0] is not None
                and result[-1][0] + len(result[-1][1]) == block):
            result[-1][1].append(payload)
        else:
            result.append((block, [] if block is None else [payload]))
    return result


def replay(fd, plan):
    syncs = []
    start = perf_counter_ns()
    for block, pages in plan:
        if block is None:
            before = perf_counter_ns()
            fcntl.fcntl(fd, fcntl.F_FULLFSYNC)
            syncs.append(perf_counter_ns() - before)
        else:
            count = (os.pwrite(fd, pages[0], block * 4096) if len(pages) == 1
                     else os.pwritev(fd, pages, block * 4096))
            if count != len(pages) * 4096:
                raise OSError("écriture incomplète dans le banc")
    elapsed = perf_counter_ns() - start
    if len(syncs) != 2:
        raise RuntimeError("les deux barrières sont requises")
    return {"total_ns": elapsed, "f1_ns": syncs[0], "f2_ns": syncs[1]}


def main():
    binary = p1["executable"]()
    results = {"architecture_rejeu_python": platform.machine(), "zones": []}
    with tempfile.TemporaryDirectory(prefix="novafs-groupes-") as directory:
        seed, normal, trial = [Path(directory) / n for n in ("base", "publie", "essai")]
        for sector in (512, 4096):
            with seed.open("wb") as file:
                file.truncate(p1["BLOCKS"] * p1["BLOCK"])
            p1["passes"](binary, "seed", seed, sector)
            shutil.copyfile(seed, normal)
            ops, _ = p1["operations"](p1["passes"](binary, "journal", normal, sector))
            plans = {name: commands(ops, name == "groupe") for name in ("unitaire", "groupe")}
            samples = {name: [] for name in plans}
            for i in range(25):
                # Inverser l’ordre à chaque paire, restaurer puis stabiliser la même base.
                order = tuple(plans) if i % 2 == 0 else reversed(plans)
                for name in order:
                    shutil.copyfile(seed, trial)
                    with trial.open("r+b") as file:
                        fcntl.fcntl(file.fileno(), fcntl.F_FULLFSYNC)
                        record = replay(file.fileno(), plans[name])
                    if trial.read_bytes() != normal.read_bytes():
                        raise RuntimeError("le rejeu ne reproduit pas la mutation du moteur")
                    p1["passes"](binary, "check", trial, sector, "nouveau")
                    samples[name].append(record)
            results["zones"].append({"secteur": sector, "soumissions_ecriture": {
                name: len(plan) - 2 for name, plan in plans.items()}, "mesures": samples})
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
