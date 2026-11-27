"""Comparer les baux de stockage et réseau de la fixture historique aux deux captures 5J."""

import argparse
from hashlib import sha256
import json
from pathlib import Path
import uuid
from contrat import Bail, Constat, verifier


def main():
    p = argparse.ArgumentParser()
    p.add_argument("archive", type=Path)
    p.add_argument("capture", type=Path)
    a = p.parse_args()
    oracle = (a.archive / "q35-duo-domaines-03/qemu/campagne.json").read_bytes()
    if (
        sha256(oracle).hexdigest()
        != "8d1fb7d5bb24a1457a899e9ded1a3e69219b87c30b8a0b6e4e30a344102a8200"
    ):
        raise ValueError("Oracle non reconnu")
    old = json.loads(oracle)["duo"]
    photo = json.loads((a.capture / "bilan.json").read_bytes())
    ((base, regs),) = photo["registres"]["unites"].items()
    base = int(base, 16)
    # Époque synthétique de relecture uniquement. Aucun PE lancé depuis ces clichés.
    epoch = uuid.uuid4().bytes
    baux = []
    for sid, did, iova in ((48, 1, 0x02000000), (56, 3, 0x10000000)):
        feuilles = tuple(
            (iova + 4096 * int(i), v & ~4095, v & 4095)
            for i, v in old["feuilles"][str(sid)].items()
        )
        baux.append(Bail(base, sid, did, feuilles))
    resultats = {}
    for nom in ("avant", "apres"):
        pages = {
            int(f.stem, 16): f.read_bytes() for f in (a.capture / nom).glob("*.bin")
        }
        const = Constat(
            base,
            regs,
            pages,
            frozenset(photo["tables_autorisees"]),
            frozenset([48, 56]),
        )
        resultats[nom] = verifier(
            epoch,
            epoch,
            [(0x400000, 0x500000)],
            [const],
            baux,
            {48, 56},
            [],
            True,
            False,
        )
    print(
        json.dumps(
            dict(
                octets=resultats,
                feuilles=[len(b.feuilles) for b in baux],
                io=photo["io"],
                namespace=photo["namespace"],
                recus=photo["recus"],
                ns_capture=photo["ns_capture"],
                pages=photo["pages"],
            ),
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
