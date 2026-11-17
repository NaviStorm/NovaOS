"""Relier chaque invocation aux observations Darwin, aux deux ELF et au moteur sous QEMU."""

import base64
import csv
import gzip
import hashlib
import json
from pathlib import Path
import re
from archiver import PROFILES, HERE


def digest(data):
    return hashlib.sha256(data).hexdigest()


def decode(field):
    return base64.b64decode(field)


def read(p):
    return json.loads(p.read_text())


def main(profiles, out):
    raw = json.load(gzip.open(HERE / "preuves/options.json.gz"))["rows"]
    results = {k: read(profiles / k / "resultats.json") for k in PROFILES}
    with (out / "matrice-macos.tsv").open("w") as f:
        w = csv.writer(f, delimiter="\t", lineterminator="\n")
        w.writerow(
            [
                "id",
                "argv",
                "locale",
                "pipe fragmenté",
                "GNU rc",
                "Nova rc",
                "GNU sha256 stdout",
                "Nova sha256 stdout",
                "GNU stderr brut base64",
                "Nova stderr brut base64",
                "oracle Darwin",
                *["moteur " + p for p in PROFILES],
            ]
        )
        for i, r in enumerate(raw):
            n, g = r["nova"], r["gnu"]
            w.writerow(
                [
                    i,
                    json.dumps(r["args"]),
                    r["lang"],
                    r["pipe_fragmente"],
                    g["rc"],
                    n["rc"],
                    digest(decode(g["stdout"])),
                    digest(decode(n["stdout"])),
                    g["stderr"],
                    n["stderr"],
                    "OK" if r["ok"] else "ÉCART",
                    *[
                        "OK" if i in res["ids_moteur"] else "hors vecteurs moteur"
                        for res in results.values()
                    ],
                ]
            )
    exact = {p: read(profiles / p / "cas-elf.json") for p in PROFILES}
    with (out / "matrice-elf.tsv").open("w") as f:
        w = csv.writer(f, delimiter="\t", lineterminator="\n")
        w.writerow(
            [
                "id",
                "argv",
                "GNU rc",
                "GNU stdout hex",
                "GNU stderr hex",
                "ELF rc",
                "ELF stdout hex",
                "ELF stderr hex",
                "refus propre à la cible",
                *PROFILES,
            ]
        )
        observed = {}
        for p in PROFILES:
            serial = (profiles / p / "sdk.serial").read_text()
            assert "SDK exemple admis sortie=0 restitution=OK" in serial
            observed[p] = {
                int(i): (int(rc), int(o), int(e))
                for i, rc, o, e in re.findall(
                    r"DD-RESULTAT (\d+) rc=(\d+) stdout=(\d+) stderr=(\d+)", serial
                )
            }
            assert len(observed[p]) == len(exact[p])
        for i, row in enumerate(exact[PROFILES[0]]):
            expected = row["attendu"]
            rc, o, e = expected
            for p in PROFILES:
                assert exact[p][i] == row and observed[p][i] == (
                    rc,
                    len(bytes.fromhex(o)),
                    len(bytes.fromhex(e)),
                )
            w.writerow(
                [
                    i,
                    json.dumps(row["args"]),
                    *(row["gnu"] or ["s.o.", "", ""]),
                    *expected,
                    row["refus_natif"],
                    *(["vérifié"] * len(PROFILES)),
                ]
            )


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profils", type=Path)
    parser.add_argument("--sortie", type=Path)
    args = parser.parse_args()
    main(args.profils, args.sortie or args.profils)
