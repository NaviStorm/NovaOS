"""Utilise un répertoire test/ de gawk 5.4.1 fourni séparément. Aucun programme GNU n'est copié dans le dépôt."""
import argparse
import json
from pathlib import Path
from differential import probe, sha

CASES = ("arrayind2", "arrayparm", "concat2", "concat4", "fieldassign", "fieldwdth",
         "fmtmix", "fnarray", "forref", "forsimp", "gsubasgn", "gsubind", "gsubtest")


def main():
    p = argparse.ArgumentParser()
    p.add_argument("tests", type=Path)
    p.add_argument("reference", type=Path)
    p.add_argument("awk", type=Path)
    p.add_argument("report", type=Path)
    args = p.parse_args()
    records = []
    for name in CASES:
        script = (args.tests / (name+".awk")).read_bytes()
        inp = args.tests / (name+".in")
        data = inp.read_bytes() if inp.exists() else b""
        a, b = [probe(str(exe.resolve()), ["-f", "cas.awk"], data, {"cas.awk":script}, "C")
                for exe in (args.reference, args.awk)]
        records.append(dict(name=name, script_sha256=sha(script), stdin_sha256=sha(data),
                            oracle=a, observed=b, differences=[k for k in a if a[k]!=b[k]]))
    args.report.write_text(json.dumps(records, indent=1)+"\n")
    for r in records:
        print(r["name"], r["differences"])


if __name__ == "__main__":
    main()
