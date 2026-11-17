"""Recalculer les médianes Mac par charge depuis le JSON brut de mesurer-performances-cmp.sh."""
import argparse
from decimal import Decimal
import hashlib
import json
from pathlib import Path
from statistics import median

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("campagne", type=Path, nargs="+")
args = parser.parse_args()
ok = True
for path in args.campagne:
    data = path.read_bytes()
    report = json.loads(data, parse_float=Decimal)
    assert report["outils"]["GNU"]["version"] == "cmp (GNU diffutils) 3.12", path
    print(f"\n{path} — SHA-256 {hashlib.sha256(data).hexdigest()}")
    print("| Taille | Traitement | Nova (ms) | GNU 3.12 (ms) | Nova/GNU | ≤ 1,10 |")
    print("|---|---|---:|---:|---:|---|")
    if not report["mesures"]:
        raise ValueError(f"Campagne vide : {path}")
    charges = {"égaux", "début", "milieu", "fin", "EOF", "limite", "deux sauts",
               "sauts distincts", "liste clairsemée", "même objet"}
    for taille in {r["taille_octets"] for r in report["mesures"]}:
        labels = [r["traitement"] for r in report["mesures"] if r["taille_octets"] == taille]
        if len(labels) != len(charges) or set(labels) != charges:
            raise ValueError(f"Charges absentes ou dupliquées à {taille} octets : {path}")
    for row in report["mesures"]:
        samples = row["mesures"]
        count = report.get("repetitions", 3)
        assert count >= 3 and all(len(samples[name]) == count for name in ("Nova", "GNU")), row
        nova, gnu = (median(m["secondes"] for m in samples[name]) for name in ("Nova", "GNU"))
        assert nova > 0 and gnu > 0, row
        passed = nova <= Decimal("1.10") * gnu
        ok = ok and passed
        size = row["taille_octets"]
        print(f"| {size // 2**20} Mio | {row['traitement']} | {1000*nova:.6f} | {1000*gnu:.6f} | "
              f"{nova/gnu:.6f} | {'oui' if passed else 'NON'} |")
raise SystemExit(0 if ok else 1)
