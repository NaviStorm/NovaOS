#!/usr/bin/env python3
"""Recalcule les distributions sur les appels (et les écarts par campagne)."""
import argparse
import csv
import gzip
import hashlib
import json
from collections import defaultdict
from pathlib import Path
from statistics import median


def dist(ordered):
    n = len(ordered)
    return {k: ordered[(n * p + 99) // 100 - 1] for k, p in
            (("p50", 50), ("p95", 95), ("p99", 99), ("max", 100))}


def analyse(folder):
    meta = json.loads((folder / "lectures.json").read_text())
    raw = folder / "lectures.csv.gz"
    with raw.open("rb") as f:
        if hashlib.file_digest(f, "sha256").hexdigest() != meta["sha256_csv_gz"]:
            raise ValueError("empreinte du journal incorrecte")
    groups = defaultdict(list)
    keys = ("architecture", "cache_hote", "phase", "moteur")
    samples = ("total_ns", "cpu_ns", "fichier_ns", "lecture_ns", "controle_liberation_ns")
    with gzip.open(raw, "rt", newline="") as f:
        for row in csv.DictReader(f):
            groups[tuple(row[k] for k in keys)].append(tuple(int(row[k]) for k in samples))

    runs = defaultdict(dict)
    for run in meta["campagnes"]:
        for phase, obs in run["phases"].items():
            key = run["architecture"], run["cache_hote"], phase, run["moteur"]
            c = run["campagne"]
            if c in runs[key]:
                raise ValueError("campagne dupliquée")
            runs[key][c] = obs
    if runs.keys() != groups.keys() or sum(map(len, groups.values())) != meta["lignes"]:
        raise ValueError("journal ou manifeste incomplet")

    summary = []
    for key, rows in sorted(groups.items()):
        rounds = runs[key]
        if sum(v["duree_appel_ns"]["n"] for v in rounds.values()) != len(rows):
            raise ValueError("effectif incorrect")
        # Tableau d'indices implicite : chaque tuple garde les mesures du MÊME
        # appel. Le temps fichier peut comprendre du CPU : ce n'est pas un addend.
        rows.sort(key=lambda r: r[0])
        cutoff = rows[(len(rows) * 99 + 99) // 100 - 1][0]
        tail = [r for r in rows if r[0] >= cutoff]  # Inclure tous les ex æquo.
        ns = sum(s["boucle_ns"] for r in rounds.values() for s in r["series"])
        summary.append({
            **dict(zip(keys, key, strict=True)), "n": len(rows),
            "latence_ns": dist([r[0] for r in rows]), "Mio_s": len(rows) * 0.0625 * 1e9 / ns,
            "appel_le_plus_lent": dict(zip(samples, rows[-1], strict=True)),
            "p99_par_campagne_ns": {i: r["duree_appel_ns"]["p99"] for i, r in rounds.items()},
            "appels_au_moins_11_738875_ms": sum(r[0] >= 11738875 for r in rows),
            "cpu_superieur_au_mur": sum(r[1] > r[0] for r in rows),
            "queue_des_appels": {"n": len(tail), **{k + "_mediane": median(r[i] for r in tail)
                                  for i, k in enumerate(samples)},
                                 "reliquat_signe_mur_cpu_ns_mediane": median(r[0] - r[1] for r in tail)},
        })
    paired = []
    for arch, policy, phase in sorted({key[:3] for key in runs}):
        for old, new in (("P3", "P4"), ("P4", "P4.1")):
            a, b = (runs[arch, policy, phase, v] for v in (old, new))
            if a.keys() != b.keys():
                raise ValueError("campagne non appariée")
            ratios = [b[c]["duree_appel_ns"]["p99"] / a[c]["duree_appel_ns"]["p99"]
                      for c in sorted(a)]
            paired.append({"architecture": arch, "cache_hote": policy, "phase": phase,
                           "avant": old, "apres": new, "ratio_p99_apres_avant": ratios,
                           "ratio_median": median(ratios), "campagnes_plus_rapides": sum(r < 1 for r in ratios)})
    return {"unites": "nanosecondes, octets utiles par 2^20 par seconde", "groupes": summary,
            "comparaisons_appariees": paired}


def markdown(analysis):
    def number(v, digits=3):
        return f"{v:.{digits}f}".replace(".", ",")
    lines = ["# NovaFS P4.1 — distributions des lectures comparées", "",
             "Quantiles au rang plafond sur les durées individuelles de six campagnes. Chaque ligne conserve",
             "une architecture, une politique de cache hôte et une température du cache NovaFS. Le débit",
             "divise les octets utiles par la durée des boucles. Les p99 par campagne et leurs ratios appariés",
             "figurent dans la [synthèse numérique](validation/v0.9.5/synthese.json).",
             "Les maxima sont conservés même lorsqu’ils n’apparaissent que dans une campagne.", ""]
    for arch in ("aarch64", "x86_64"):
        lines += ["## " + ("ARM64 natif" if arch == "aarch64" else "x86_64 sous Rosetta"), "",
                  "| Cache hôte | Cache NovaFS | Version | n | p50 ms | p95 ms | p99 ms | max ms | Mio/s |",
                  "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |"]
        for policy in ("buffered", "nocache"):
            for phase in ("froid_32m", "chaud_512k"):
                for g in analysis["groupes"]:
                    if (g["architecture"], g["cache_hote"], g["phase"]) != (arch, policy, phase):
                        continue
                    cols = ["ordinaire" if policy == "buffered" else "F_NOCACHE",
                            "froid 32 Mio" if phase == "froid_32m" else "chaud 512 Kio",
                            g["moteur"], str(g["n"]),
                            *(number(g["latence_ns"][k] / 1e6) for k in ("p50","p95","p99","max")),
                            number(g["Mio_s"], 2)]
                    lines.append("| " + " | ".join(cols) + " |")
        lines += [""]
    lines += ["[Protocole, interprétation et limites](novafs-p4.1-latences.md) · "
              "[Échantillons individuels](validation/v0.9.5/lectures.csv.gz)"]
    return "\n".join(lines)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("dossier", type=Path)
    p.add_argument("--markdown", action="store_true", help="produire le tableau lisible en millisecondes")
    args = p.parse_args()
    result = analyse(args.dossier)
    print(markdown(result) if args.markdown else json.dumps(result, ensure_ascii=False, indent=2))
