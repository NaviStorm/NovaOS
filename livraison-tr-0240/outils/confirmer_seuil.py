"""Comparer deux séries Darwin et les empreintes de référence des charges communes."""
import argparse
import json
import statistics
from pathlib import Path
from mesures import workloads, MODES

ROOT = Path(__file__).resolve().parents[1]


def read(path):
    return json.loads(path.read_text())


def summary(series, tool):
    values = [sample["seconds"] * 1000 for sample in series["samples"][tool]]
    return f"{statistics.median(values):.3f} [{min(values):.3f}–{max(values):.3f}]"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dossier", type=Path)
    folder = parser.parse_args().dossier
    history = {}
    for name in ("mesures-mac.json", "recette-courte-2026-11-17/mesures.json"):
        for row in read(ROOT / "preuves" / name)["rows"]:
            history[row["size"], row["mode"]] = row
    print("Les durées sont les médianes en millisecondes, suivies du minimum et du maximum. Chaque série est jugée sur les médianes non arrondies.")
    print("\n| Charge | Arguments | Entrée / sortie (octets) | A Nova (ms) | A GNU (ms) | A Nova/GNU | B Nova (ms) | B GNU (ms) | B Nova/GNU | Verdict | SHA-256 sortie |")
    print("|---|---|---:|---:|---:|---:|---:|---:|---:|---|---|")
    failures = 0
    identities = None
    for profile in ("court", "complet"):
        a, b = (read(folder / f"{series}-{profile}.json") for series in ("A", "B"))
        assert a["sha256"] == b["sha256"] and a["versions"] == b["versions"]
        if identities is None:
            identities = a["sha256"]
        assert a["sha256"] == identities
        expected = [(label, mode, len(prefix) + len(pattern) * n)
                    for label, prefix, pattern, n, modes in workloads(profile == "complet") for mode in modes]
        for (label, mode, size), row, other in zip(expected, a["rows"], b["rows"], strict=True):
            result = []
            passed = True
            for report, check, first in [(a, row, "Nova"), (b, other, "GNU")]:
                assert (check["size"], check["mode"], check["input"], check["args"]) == (label, mode, size, MODES[mode])
                assert check["verification"]["Nova"] == check["verification"]["GNU"] == dict(
                    status=0, stderr_hex="", output=check["output"], sha256_output=check["sha256_output"])
                second = "GNU" if first == "Nova" else "Nova"
                order = []
                times = {}
                for index in range(report["passages"]):
                    order.extend((first, second) if index % 2 == 0 else (second, first))
                assert check["order"] == order and report["passages"] >= 3
                for tool in ("Nova", "GNU"):
                    samples = check["samples"][tool]
                    assert len(samples) == report["passages"] and all(s["seconds"] > 0 for s in samples)
                    times[tool] = statistics.median(s["seconds"] for s in samples)
                ratio = times["Nova"] / times["GNU"]
                passed = passed and times["Nova"] <= 1.10 * times["GNU"]
                result += [summary(check, "Nova"), summary(check, "GNU"), f"{ratio:.5f}"]
            for key in ("input", "output", "input_sha256", "sha256_output"):
                assert row[key] == other[key]
                if (label, mode) in history:
                    assert row[key] == history[label, mode][key]
            failures += int(not passed)
            print(f"| {label} | `{' '.join(row['args'])}` | {size} / {row['output']} | "
                  + " | ".join(result) + f" | {'respecté' if passed else 'dépassé'} | `{row['sha256_output']}` |")
    print(f"\n{failures} charge(s) dépassent le plafond dans au moins une des deux séries.")
    return int(failures != 0)


if __name__ == "__main__":
    raise SystemExit(main())
