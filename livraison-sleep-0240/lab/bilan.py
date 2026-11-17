"""Réunit les résultats bruts GNU, Darwin, ELF et nsh. N’exécute pas les utilitaires."""

import argparse
import csv
import gzip
import json
import math
import statistics
from pathlib import Path


def read(path):
    with gzip.open(path, "rt") if path.suffix == ".gz" else path.open() as f:
        return json.load(f)


def measures(rows):
    groups = {}
    for row in rows:
        if not row["id"].startswith("mesure-"):
            continue
        _, arg, index = row["id"].split("-")
        if index == "0":
            continue
        groups.setdefault(arg, []).append(row)
    for arg, group in groups.items():
        times = sorted(r["temps_s"] * 1000 for r in group)
        p95 = times[math.ceil(0.95 * len(times)) - 1]
        yield (
            arg,
            len(group),
            statistics.median(times),
            p95 - 1000 * float(arg),
            statistics.median(r["cpu_ticks"] for r in group),
            max(r["cpu_ticks"] for r in group),
        )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "--preuves", type=Path, default=Path(__file__).resolve().parents[1] / "preuves"
    )
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    a.out.mkdir(exist_ok=True, parents=True)
    host = read(a.preuves / "options.json")["cas"]
    elf = {f.name[:-8]: read(f)["cas"] for f in sorted(a.preuves.glob("sdk-*.json.gz"))}
    shell = {
        f.name[:-8]: read(f)["cas"] for f in sorted(a.preuves.glob("nsh-*.json.gz"))
    }
    assert len(elf) == len(shell) == 4
    with (a.out / "matrice.tsv").open("w", newline="") as f:
        w = csv.writer(f, delimiter="\t", lineterminator="\n")
        w.writerow(
            [
                "profil",
                "cas",
                "argv_hex",
                "locale",
                "POSIX",
                "demande_s",
                "statut",
                "stdout_hex",
                "stderr_hex",
                "temps_s",
                "validation",
            ]
        )
        for r in host:
            for key in ("gnu", "nova"):
                m = r[key]
                w.writerow(
                    [
                        key + "-macos",
                        r["cas"],
                        " ".join(r["argv"]),
                        r["locale"],
                        r["posix"],
                        r["demande_s"],
                        m["rc"],
                        m["stdout"],
                        m["stderr"],
                        m["ns"] / 1e9,
                        r["ok"],
                    ]
                )
        for profile, rows in elf.items():
            for r in rows:
                w.writerow(
                    [
                        profile,
                        r["id"],
                        " ".join(r["argv"]),
                        "C / ARC-026",
                        False,
                        r["demande_s"],
                        r["statut"],
                        r["stdout"],
                        r["stderr"],
                        r["temps_s"],
                        r["ok"],
                    ]
                )
    with (a.out / "nsh.tsv").open("w", newline="") as f:
        w = csv.writer(f, delimiter="\t", lineterminator="\n")
        w.writerow(["profil", "langue ou parcours", "commande ou arguments", "statut"])
        for profile, rows in shell.items():
            for tag, cmd, rc in rows:
                w.writerow([profile, tag, cmd, rc])
    with (a.out / "performances-qemu.tsv").open("w", newline="") as f:
        w = csv.writer(f, delimiter="\t", lineterminator="\n")
        w.writerow(
            [
                "profil",
                "demande_s",
                "passages",
                "médiane_ms",
                "retard_p95_ms",
                "CPU_ticks_médiane",
                "CPU_ticks_max",
            ]
        )
        for profile, rows in elf.items():
            for arg, n, median, p95, cpu, peak in measures(rows):
                w.writerow(
                    [profile, arg, n, round(median, 3), round(p95, 3), cpu, peak]
                )


if __name__ == "__main__":
    main()
