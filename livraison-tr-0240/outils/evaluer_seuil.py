"""Vérifier le plafond par charge à partir des passages bruts du banc Darwin."""
import argparse
import json
from pathlib import Path
import statistics


def evaluate(path):
    report = json.loads(path.read_text())
    print(f"\n{path} — {report['versions']['Nova']} / {report['versions']['GNU']}")
    print(f"Nova SHA-256 {report['sha256']['Nova']}\nGNU SHA-256  {report['sha256']['GNU']}")
    print("| Charge | Arguments tr | Entrée (octets) | Sortie (octets) | Nova (ms) | GNU (ms) | Nova/GNU | Plafond 1,10 | SHA-256 sortie |")
    print("|---|---|---:|---:|---:|---:|---:|---|---|")
    failures = 0
    if not report["rows"]:
        raise ValueError("rapport vide")
    for row in report["rows"]:
        times = {}
        for tool in ("Nova", "GNU"):
            samples = [s["seconds"] for s in row["samples"].get(tool, [])]
            if len(samples) < 3 or any(t <= 0 for t in samples):
                raise ValueError(f"{row['size']} / {tool} : trois durées positives requises")
            times[tool] = statistics.median(samples)
        nova, gnu = times["Nova"], times["GNU"]
        ok = nova <= 1.10 * gnu
        failures += int(not ok)
        args = " ".join(row["args"])
        print(f"| {row['size']} | `{args}` | {row['input']} | {row['output']} | "
              f"{nova * 1000:.4f} | {gnu * 1000:.4f} | {nova / gnu:.5f} | "
              f"{'respecté' if ok else 'dépassé'} | `{row['sha256_output']}` |")
    print(f"\n{failures} dépassement(s) sur {len(report['rows'])} charges.")
    return failures


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rapports", type=Path, nargs="+")
    failures = sum(evaluate(path) for path in parser.parse_args().rapports)
    raise SystemExit(int(failures != 0))
