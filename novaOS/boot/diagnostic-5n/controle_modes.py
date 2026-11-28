"""Contre-épreuve de la politique OVMF sur le même GPT. Aucun exporteur et aucune modification de db."""

import argparse
import json
from pathlib import Path
import sys

from demarrage import HORS_DB, boot
from support import OS, PE_SHA, public_member, require

sys.path.insert(0, str(OS / "tests/ethernet_v1"))
from port_qemu import qualification


def control(image, firmware, temp):
    temp.mkdir(mode=0o700)
    data = image.read_bytes()
    good = public_member("valide.efi", PE_SHA)
    denied = public_member("hors-db.efi", HORS_DB)
    require(len(denied) == len(good) and data.count(good) == 1, "Témoin sur l’ESP")
    p = temp / "temoin.img"
    p.write_bytes(data.replace(good, denied))
    with qualification(Path(__file__).resolve().parents[4], {}), p.open("rb") as f:
        active = boot(temp / "actif", firmware, "actif", f, temoin=True)
        cal = boot(temp / "cal", firmware, "inactif", f, temoin=True)
        inactive = boot(
            temp / "inactif", firmware, "inactif", f, cal["base"], temoin=True
        )
    return dict(actif=active, inactif=inactive)


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    for n in ("image", "firmware", "travail"):
        ap.add_argument(n, type=Path)
    a = ap.parse_args()
    print(
        json.dumps(
            control(a.image.resolve(), a.firmware.resolve(), a.travail.resolve()),
            indent=2,
        )
    )
