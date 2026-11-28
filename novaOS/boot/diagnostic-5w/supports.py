"""Fichiers ordinaires du banc. Le firmware OVMF et son gabarit restent en lecture seule."""

import argparse
import subprocess
import sys
import tarfile
from pathlib import Path

from virt.firmware.varstore.edk2 import Edk2VarStore

NOVA = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(NOVA / "boot/diagnostic-5m"), str(NOVA / "boot/diagnostic-5o")]
from image import pe_info
from image5o import creer, lecteur, verifier

OWNER = "45434150-cdbe-4c63-8fcd-a45fa42834df"


def preparer(efi, gabarit, sortie):
    sortie.mkdir(mode=0o700, parents=True, exist_ok=False)
    with lecteur(efi) as f:
        pe = f.read()
    with lecteur(gabarit):
        pass
    archive = NOVA / "docs/validation/diagnostic-uefi-5h/supports.tar.gz"
    with tarfile.open(archive) as membres:
        for nom in ("PK.pem", "KEK.pem"):
            (sortie / nom).write_bytes(membres.extractfile(nom).read())
    h = pe_info(pe)["authenticode"]
    outil = Path(sys.executable).parent / "virt-fw-vars"
    subprocess.run(
        [
            outil,
            "-i",
            gabarit,
            "-o",
            sortie / "actif.fd",
            "--set-pk",
            OWNER,
            sortie / "PK.pem",
            "--add-kek",
            OWNER,
            sortie / "KEK.pem",
            "--microsoft-kek",
            "none",
            "--add-db-hash",
            OWNER,
            h,
            "--secure-boot",
        ],
        check=True,
    )
    store = Edk2VarStore(str(sortie / "actif.fd"))
    var = store.get_varlist()
    var.set_bool("SecureBootEnable", False)
    store.write_varstore(str(sortie / "inactif.fd"), var)
    md = (NOVA / "boot/diagnostic-5e/vecteurs/ioapic.toml").read_bytes()
    if pe.count(md) != 1:
        raise ValueError("Manifeste témoin ambigu")
    invalide = pe.replace(md, md.replace(b"diagnostic", b"diagnostix", 1))
    if pe == invalide:
        raise ValueError("Substitution absente")
    for nom, b in (("standard", pe), ("altere", invalide)):
        p = sortie / (nom + ".img")
        creer(p, 68 << 20, b)
        with lecteur(p) as src:
            verifier(src, 68 << 20, b)


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    for nom in ("efi", "gabarit", "sortie"):
        ap.add_argument(nom, type=Path)
    a = ap.parse_args()
    preparer(a.efi, a.gabarit, a.sortie)
