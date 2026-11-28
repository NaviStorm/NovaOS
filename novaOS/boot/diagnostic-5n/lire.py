"""Afficher les diagnostics validés d'une copie d'image. Le fichier est ouvert en lecture seule."""

import argparse
import json
from pathlib import Path

from journaux import history, read_at
from support import require, verify


def lire(path, as_json):
    require(path.is_file() and not path.is_symlink(), "Copie régulière attendue")
    with path.open("rb") as f:
        areas = verify(f)
        states = history(f, areas)
        for i, (state, _) in enumerate(states):
            slot = f"BOOT{i + 1:04d}"
            if state == "valide":
                if as_json:
                    yield json.dumps(
                        dict(
                            emplacement=slot,
                            rapport=json.loads(read_at(f, areas[i][0])),
                        ),
                        ensure_ascii=False,
                    )
                else:
                    yield slot + "\n" + read_at(f, areas[i][1]).decode("utf-8").rstrip()
            elif state == "interrompu":
                yield (
                    json.dumps(dict(emplacement=slot, etat=state))
                    if as_json
                    else slot + " | publication interrompue"
                )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("image", type=Path)
    p.add_argument("--json", action="store_true")
    args = p.parse_args()
    for line in lire(args.image, args.json):
        print(line)
