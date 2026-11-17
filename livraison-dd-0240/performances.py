"""Mesurer un processus à la fois, de son lancement à wait4, puis relire sa sortie."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tempfile
import time
from options import GNU, NOVA


def digest(p):
    with p.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--rapide", action="store_true")
    g.add_argument("--complet", action="store_true")
    p.add_argument("rapport", type=Path)
    a = p.parse_args()
    size = (32 if a.complet else 4) * 1024**2
    passes = 5 if a.complet else 3
    raw = []
    summary = []
    assert platform.system() == "Darwin" and platform.machine() == "arm64"
    oracle = subprocess.check_output([GNU, "--version"], text=True).splitlines()[0]
    assert "9.12" in oracle
    temp = Path(os.environ["TMPDIR"])
    with (
        (temp.parent / "arc027-mesures.lock").open("a") as campaign,
        (temp.parent / "cut-performances.lock").open("a") as lock,
        tempfile.TemporaryDirectory(dir=temp, prefix="dd-mesure-") as dir_,
    ):
        fcntl.flock(campaign, fcntl.LOCK_EX)
        fcntl.flock(lock, fcntl.LOCK_EX)
        root = Path(dir_)
        largest = max(size, 100 * 1024**2 if a.complet else 0)
        assert shutil.disk_usage(root).free >= 3 * largest + 64 * 1024**2
        source = root / "temoin"
        target = root / "sortie"
        with source.open("wb") as f:
            for _ in range(size // 65536):
                f.write(bytes(range(256)) * 256)
        hashes = {("fichier", size): digest(source)}
        for n in {size, largest}:
            h = hashlib.sha256()
            for _ in range(n // 65536):
                h.update(bytes(65536))
            hashes["zero", n] = h.hexdigest()
        workloads = [
            (label, bs, size)
            for label in ("fichier", "zero")
            for bs in (512, 65536, 1048576)
        ]
        if a.complet:
            workloads.append(("zero", 100 * 1024**2, 100 * 1024**2))
        for label, block, n in workloads:
            for i in range(passes + 1):
                tools = [("Nova", NOVA), ("GNU", GNU)]
                for tool, exe in tools[:: 1 if i % 2 == 0 else -1]:
                    target.unlink(missing_ok=True)
                    args = [
                        str(exe),
                        "if=" + (str(source) if label == "fichier" else "/dev/zero"),
                        "of=" + str(target),
                        "bs=" + str(block),
                        "count=" + str(n // block),
                        "status=none",
                    ]
                    start = time.monotonic_ns()
                    proc = subprocess.Popen(
                        args,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        env=dict(os.environ, LC_ALL="C"),
                    )
                    pid, status, usage = os.wait4(proc.pid, 0)
                    ns = time.monotonic_ns() - start
                    proc.returncode = os.waitstatus_to_exitcode(status)
                    stdout, stderr = proc.communicate()
                    sha = digest(target)
                    assert (
                        pid == proc.pid
                        and proc.returncode == 0
                        and not stdout
                        and not stderr
                        and target.stat().st_size == n
                        and sha == hashes[label, n]
                    )
                    raw.append(
                        dict(
                            source=label,
                            bs=block,
                            octets=n,
                            outil=tool,
                            passage=i,
                            ns=ns,
                            rss=usage.ru_maxrss,
                            sha256=sha,
                        )
                    )
            row = dict(source=label, bs=block, octets=n)
            for tool in ["Nova", "GNU"]:
                cells = [
                    r
                    for r in raw
                    if (r["source"], r["bs"], r["octets"], r["outil"])
                    == (label, block, n, tool)
                    and r["passage"]
                ]
                ns = statistics.median(r["ns"] for r in cells)
                row[tool] = dict(
                    ms=ns / 1e6,
                    MiBs=n * 1e9 / (ns * 1024**2),
                    MiB=statistics.median(r["rss"] for r in cells) / 1024**2,
                )
            summary.append(row)
            print(
                label,
                block,
                n,
                *(
                    f"{row[k]['ms']:.2f} ms, {row[k]['MiB']:.2f} Mio"
                    for k in ("Nova", "GNU")
                ),
                flush=True,
            )
    a.rapport.write_text(
        json.dumps(
            dict(
                systeme=platform.platform(),
                processeur=subprocess.check_output(
                    ["sysctl", "-n", "machdep.cpu.brand_string"], text=True
                ).strip(),
                oracle=oracle,
                passes=passes,
                binaires={str(p): digest(p) for p in (NOVA, GNU)},
                brut=raw,
                mediane=summary,
            ),
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
