#!/usr/bin/env python3
"""Relit les séries P4 publiées sans reconstituer de faux échantillons."""
import json
from pathlib import Path


def rank(n, p):
    return (n * p + 99) // 100


def bounds(series):
    # Pour chaque rang inconnu, le précédent quantile donne une borne basse
    # et le suivant une borne haute. Toute observation est non négative.
    lower, upper = [], []
    for row in series:
        n = row["n"]
        known = [(rank(n, p), row[key]) for p, key in
                 ((50, "p50_ns"), (95, "p95_ns"), (99, "p99_ns"), (100, "max_ns"))]
        for ordinal in range(1, n + 1):
            lower.append(max((v for r, v in known if r <= ordinal), default=0))
            upper.append(min(v for r, v in known if r >= ordinal))
    idx = rank(len(lower), 99) - 1
    return [sorted(lower)[idx], sorted(upper)[idx]]


def extract(path):
    groups = json.loads(path.read_text())["campagnes"]
    case, = (g for g in groups if g["scenario"] == "complet")
    series = []
    for line in case["journal"]:
        fields = line.split("\t")
        if fields[0] == "lecture_seq64k":
            s, n, size, elapsed, p50, p95, p99, maximum, reads, writes, flushes, fns, _ = map(int, fields[1:])
            if writes or flushes or fns or n != 512 or size != n * 65536:
                raise ValueError("périmètre de la série inattendu")
            series.append(dict(serie=s, n=n, octets=size, duree_ns=elapsed,
                               p50_ns=p50, p95_ns=p95, p99_ns=p99, max_ns=maximum,
                               blocs_lus=reads))
    if [s["serie"] for s in series] != list(range(1, 6)):
        raise ValueError("séries historiques incomplètes")
    return {"series": series, "pire_p99_par_serie_ns": max(s["p99_ns"] for s in series),
            "bornes_du_p99_global_ns": bounds(series),
            "debit_Mio_s": sum(s["octets"] for s in series) * 1e9 /
                           (2**20 * sum(s["duree_ns"] for s in series))}


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[1] / "docs/validation/v0.9.4"
    print(json.dumps({f"{rev}-{arch}": extract(root / f"performance-{rev}-{arch}.json")
                      for rev in ("P3", "P4") for arch in ("aarch64", "x86_64")},
                     indent=2, ensure_ascii=False))
