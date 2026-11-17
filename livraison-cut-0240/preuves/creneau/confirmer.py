"""Recalculer les deux séries finales à partir des passages bruts et de leurs SHA."""
from pathlib import Path
import hashlib
import json
import statistics

ROOT = Path(__file__).resolve().parent
NAMES = ("Publié", "Nova", "GNU")
BINARIES = (ROOT.parents[1] / "bin/cut-darwin-arm64", ROOT / "cut-darwin-r3",
            Path("/opt/homebrew/bin/gcut"))


def signature(row):
    return (row["taille"], " ".join(row["arguments"]), row["entree"], row["structure"], row["locale"])


def rows_for(series):
    rows = {}
    for family, count in (("tsv", 18), ("longues", 15)):
        path = ROOT / f"r3-{family}-{series}.json"
        data = json.loads(path.read_text())
        assert data["serie"] == series and len(data["essais"]) == count, path
        for name, binary in zip(NAMES, BINARIES):
            assert hashlib.sha256(binary.read_bytes()).hexdigest() == data["binaires"][name]["sha256"], binary
        assert data["binaires"]["GNU"]["version"] == "cut (GNU coreutils) 9.12"
        for row in data["essais"]:
            key = signature(row)
            assert key not in rows
            times = []
            order = row["ordre"]
            names = ["Nova", "GNU", "Publié"]
            rounds = 9 if row["octets"] < 100_000_000 else 3
            assert len(order) == rounds
            for i, sequence in enumerate(order):
                start = (i + series - 1) % len(names)
                assert sequence == names[start:] + names[:start]
            for name in NAMES:
                tool = row["outils"][name]
                assert tool["sortie"] == row["attendu"] and tool["statut_validation"] == tool["stderr_octets"] == 0
                assert tool["echauffement"]["statut"] == 0 and len(tool["passages"]) == rounds
                assert all(rep["statut"] == 0 and rep["rss_octets"] > 0 and rep["s"] > 0 for rep in tool["passages"])
                median = statistics.median(rep["s"] for rep in tool["passages"])
                assert median == tool["mediane_s"]
                times.append(median)
            assert row["ratio_nova_gnu"] == times[1] / times[2]
            assert row["respecte_plafond"] == (times[1] <= 1.10 * times[2])
            rows[key] = (row, times)
    return rows


def main():
    first, second = rows_for(1), rows_for(2)
    assert first.keys() == second.keys()
    print("Temps médians en secondes ; comparaisons sur les valeurs non arrondies.")
    print("Charge | Entrée | Octets | Publié 1 | Nova 1 | GNU 1 | N/G 1 | Publié 2 | Nova 2 | GNU 2 | N/G 2 | Verdict")
    failures = 0
    for key, (one, times1) in first.items():
        two, times2 = second[key]
        assert (one["sha256"], one["attendu"], one["octets"]) == (two["sha256"], two["attendu"], two["octets"])
        for rows in (first, second):
            if key[2] == "stdin":
                twin = rows[(key[0], key[1], "fichier", key[3], key[4])][0]
                assert (twin["sha256"], twin["attendu"]) == (one["sha256"], one["attendu"])
        verdict = one["respecte_plafond"] and two["respecte_plafond"]
        failures += not verdict
        cells = []
        for times in (times1, times2):
            cells.extend(f"{t:.6f}" for t in times)
            cells.append(f"{times[1] / times[2]:.4f}")
        print(f"{key[0]} {key[1]} {key[3]} | {key[2]} | {one['octets']} | " +
              " | ".join(cells) + (" | OK" if verdict else " | DÉPASSÉ"))
    print(f"{len(first)-failures}/{len(first)} charges passent dans les deux séries.")
    return int(failures > 0)


if __name__ == "__main__":
    raise SystemExit(main())
