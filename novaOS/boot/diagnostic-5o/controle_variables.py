"""Relire PK, KEK, dbx et db dans les flashes synthétiques après arrêt des VM."""

import argparse
import json
from pathlib import Path
from magasin import Edk2VarStore, comparer_magasin, pe_info, chargeur, require_original


def controle(b):
    ref = b / "magasin/original.fd"
    nouveau = b / "magasin/actif.fd"
    require_original(ref)
    diff = comparer_magasin(ref, nouveau, [pe_info(chargeur())["authenticode"]])

    def contenu(v):
        return (v.guid, v.attr, v.count, v.pkidx, v.time, v.data)

    out = {}
    for name in json.loads((b / "usb-a/bilan.json").read_text()):
        if "-inactif-" in name:
            continue
        folder = b / "usb-a" / name
        if name.startswith("ambigu-") and (b / "usb-deux").is_dir():
            folder = b / "usb-deux" / name
        before = Edk2VarStore(
            str(ref if name.startswith("hors-db-") else nouveau)
        ).get_varlist()
        after = Edk2VarStore(str(folder / "variables.fd")).get_varlist()
        ok = {
            n: contenu(before[n]) == contenu(after[n])
            for n in ("PK", "KEK", "dbx", "db")
        }
        if not all(ok.values()):
            raise ValueError("Politique persistée différente après " + name)
        out[name] = ok
    require_original(ref)
    return dict(original=diff["original"], copie=diff["copie"], apres_arret=out)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("build", type=Path)
    print(json.dumps(controle(p.parse_args().build), indent=2))
