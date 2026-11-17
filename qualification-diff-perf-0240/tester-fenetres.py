"""Comparer les sorties et statuts du candidat, du binaire publié et de GNU."""
from pathlib import Path
import argparse
import json
import os
import random
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "livraison-diff-0240"))
from laboratoire import TMP, NOVA, execute, digest, save


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("candidat", type=Path)
    parser.add_argument("oracle", type=Path)
    parser.add_argument("rapport", type=Path)
    args = parser.parse_args()
    binaries = {"publie": NOVA, "candidat": args.candidat.resolve(), "GNU": args.oracle.resolve()}
    rows = []
    with tempfile.TemporaryDirectory(prefix="diff-reg-fenetres-", dir=TMP) as tmp:
        d = Path(tmp)
        def case(name, a, b, opts=(), missing=False):
            for file, data in ((d/"a",a), (d/"b",b)):
                file.write_bytes(data)
                os.utime(file, (1700000000,1700000000))
            if missing:
                (d/"b").unlink()
            result = {k:execute(exe,[*opts,"a","b"],d) for k,exe in binaries.items()}
            same = result["publie"] == result["candidat"]
            rows.append(dict(nom=name, options=opts, identique_publie=same,
                             identique_GNU=result["candidat"]==result["GNU"],
                             statut=result["candidat"][0], resultat=result if not same else None))
        rng = random.Random(27110)
        lines = [f"{rng.randrange(10**9):09d} élément ordinaire {i:05d}\n".encode() for i in range(512)]
        a = b"".join(lines)
        for i in [0, 1, 255, 510, 511]:
            for op, replacement in [("change",[b"autre\n"]), ("ajout",[b"autre\n",lines[i]]), ("retrait",[])]:
                case(f"{op}-{i}",a,b"".join(lines[:i]+replacement+lines[i+1:]))
        for n in [0, 511, 512, 513, 8191, 8192, 8193, 11000]:
            case(f"sans-LF-{n}",b"x"*n+b"q",b"x"*n+b"p")
            case(f"suffixe-milieu-{n}",b"x"+b"q"*n+b"\n"+a,b"q"*n+b"\n"+a)
        for old,new in [(b"a",b"a\n"),(b"",b"x"),(b"\n",b""),(b"",b"\n\n"),(b"c",b"abc")]:
            case("fin-de-fichier", a+old, a+new)
        for mode in ["distincts","déplacés","doublons","banals","consécutifs"]:
            left = lines.copy() if mode!="banals" else [lines[i%5] for i in range(512)]
            right=left.copy()
            for i in range(1,512,19):
                right[i]=b"remplacement\n"
            if mode=="déplacés": right[20]=left[1]
            if mode=="doublons": right[39]=left[2]
            if mode=="consécutifs": right[2]=right[1]
            for flags in [[],["-i"],["-B"],["-I","remplacement"],["--horizon-lines=2"],["-d"],["-H"],["-u"]]:
                case(mode,b"".join(left),b"".join(right),flags)
        case("statut-2",a,a,missing=True)
    save(args.rapport,dict(binaires={k:digest(v) for k,v in binaries.items()},cas=rows))
    print(len(rows),"cas,",sum(not r["identique_publie"] for r in rows),"écarts au publié,",
          sum(not r["identique_GNU"] for r in rows),"écarts GNU")
    if any(not r["identique_publie"] or not r["identique_GNU"] for r in rows):raise SystemExit(1)


if __name__ == "__main__":
    main()
