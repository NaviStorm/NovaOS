"""Relire des essais P9 mode 25 sans mélanger le temps du fichier et celui du disque."""
import argparse
import ast
import hashlib
import json
import math
from pathlib import Path
import re
import statistics


def one(pattern, text):
    matches = re.findall(pattern, text, re.MULTILINE)
    if len(matches) != 1:
        raise ValueError(f"{len(matches)} occurrences pour {pattern}")
    return matches[0]


def sample(text, label, n):
    hz, raw = one(rf"^NET ARM {label}(?: secteur=\d+)? hz=(\d+)(?: octets=4096)? cycles=(\[.*?\])$", text)
    values = ast.literal_eval(raw)
    if len(values) != n or min(values) <= 0 or int(hz) <= 0:
        raise ValueError(f"Série {label} incomplète")
    return int(hz), values


def read_run(path):
    record = json.loads((path / "campagne.json").read_text())
    serial = (path / "serie.log").read_bytes()
    text = serial.decode().replace("\r", "")
    if not (record["succes"] and record["bilan_invite"] and record["bail_nic"] is False
            and record["generation"] == 7 and "NET ARM bilan Ok(())" in text
            and "BLOC bilan mode=25 service=Exit(0) bloc=Exit(0) pilote=Exit(0) client=Exit(0) retrait=Ok(())" in text):
        raise ValueError(f"Parcours P9 mode 25 incomplet dans {path}")
    hz, reads = sample(text, "lectures", 64)
    flush_hz, flushes = sample(text, "flush", 16)
    if hz != flush_hz or hz != int(one(r"^NET ARM service_p9 cycles=\d+ hz=(\d+)$", text)):
        raise ValueError("Fréquences invité incohérentes")
    file_cycles = {op: int(one(rf"^ECHANTILLON ech {op} cycles=(\d+)$", text)) for op in ("write", "sync", "read")}
    if min(file_cycles.values()) <= 0:
        raise ValueError("Durée de fichier invalide")
    return dict(essai=path.name, elf=record["elf_sha256"], secteur=record["secteur"],
                source=record["volume_sha256"], serie_sha256=hashlib.sha256(serial).hexdigest(),
                p9_relu_sha256=record["p9_relu_sha256"], hz=hz, lectures_4K=reads, flushes=flushes,
                fichier_4231=file_cycles, execution_s=record["execution_s"], attente_s=record["attente_s"])


def stats(values):
    ordered = sorted(values)
    return dict(n=len(values), p50=statistics.median(ordered),
                p95=ordered[math.ceil(.95 * len(ordered)) - 1],
                p99=ordered[math.ceil(.99 * len(ordered)) - 1])


def bilan(paths):
    runs = [read_run(p) for p in paths]
    if len({(r["elf"], r["secteur"], r["source"], r["hz"]) for r in runs}) != 1:
        raise ValueError("Mélange de profils, d'ELF, de volumes initiaux ou d'horloges")
    if len({p.resolve() for p in paths}) != len(paths):
        raise ValueError("Essai compté deux fois")
    hz = runs[0]["hz"]
    flush_us = [[c * 1_000_000 / hz for c in r["flushes"]] for r in runs]
    logical_bytes = 4231 * len(runs)
    def rate(n, cycles):
        return n * hz / cycles / (1 << 20)
    return dict(essais=runs,
                lecture_bloc_Mio_s=rate(4096 * 64 * len(runs), sum(sum(r["lectures_4K"]) for r in runs)),
                lecture_fichier_Mio_s=rate(logical_bytes, sum(r["fichier_4231"]["read"] for r in runs)),
                ecriture_fichier_sync_Mio_s=rate(logical_bytes, sum(r["fichier_4231"]["write"] + r["fichier_4231"]["sync"] for r in runs)),
                flush_us=stats([t for serie in flush_us for t in serie]),
                premiers_flush_us=[serie[0] for serie in flush_us],
                flush_suivants_us=stats([t for serie in flush_us for t in serie[1:]]))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("essais", type=Path, nargs="+")
    print(json.dumps(bilan(p.parse_args().essais), ensure_ascii=False, indent=2))
