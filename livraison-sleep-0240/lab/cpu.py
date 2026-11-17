"""Mesurer l’usage CPU du seul enfant pendant un intervalle intérieur à sleep."""

from pathlib import Path
import os
import subprocess as sp
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "novaOS/tests"))
from cpu_hote_utm import CpuMoteur


def during_wait(pair, repetitions):
    samples = {label: [] for label, _ in pair}
    for turn in range(repetitions):
        for label, executable in pair if turn % 2 == 0 else reversed(pair):
            with sp.Popen(
                ["sleep", "inf"],
                executable=executable,
                stdin=sp.DEVNULL,
                stdout=sp.DEVNULL,
                stderr=sp.DEVNULL,
                env=dict(os.environ, LC_ALL="C"),
            ) as child:
                try:
                    cpu = CpuMoteur(child.pid)
                    time.sleep(0.050)
                    before = cpu.relever()
                    start = time.monotonic_ns()
                    time.sleep(0.100)
                    after = cpu.relever()
                    elapsed = time.monotonic_ns() - start
                    delta = cpu.difference(before, after)
                    samples[label].append(dict(fenetre_ns=elapsed, **delta))
                finally:
                    child.terminate()
                    try:
                        child.wait(timeout=2)
                    except sp.TimeoutExpired:
                        child.kill()
                        child.wait()
                        raise
    return samples
