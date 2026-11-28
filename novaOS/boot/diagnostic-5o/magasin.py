"""Copie synthétique du magasin OVMF 5M. Une seule empreinte Authenticode en plus dans db."""

import argparse
import json
from pathlib import Path
import sys
import uuid
from image5o import ROOT, chargeur
from demarrage import STORES, public_member

sys.path.insert(0, str(ROOT / "boot/diagnostic-5m"))
from supports import Edk2VarStore, comparer_magasin, OWNER, pe_info


def require_original(path):
    if path.read_bytes() != public_member("actif.fd", STORES["actif"]):
        raise ValueError("Magasin 5M original différent")


def preparer(out):
    out.mkdir(mode=0o700)
    original = out / "original.fd"
    actif = out / "actif.fd"
    reference = public_member("actif.fd", STORES["actif"])
    original.write_bytes(reference)
    store = Edk2VarStore(str(original))
    vs = store.get_varlist()
    h = pe_info(chargeur())["authenticode"]
    if not vs["db"].sigdb.add_hash(uuid.UUID(OWNER), bytes.fromhex(h)):
        raise ValueError("Empreinte déjà présente")
    vs["db"].data = bytes(vs["db"].sigdb)
    store.write_varstore(str(actif), vs)
    result = comparer_magasin(original, actif, [h])
    require_original(original)
    (out / "diff.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie", type=Path)
    preparer(p.parse_args().sortie)
