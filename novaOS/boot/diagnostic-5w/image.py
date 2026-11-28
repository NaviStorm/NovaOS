"""Image STANDARD GPT/FAT32 pour fichier ordinaire, avec PE fourni explicitement."""

import argparse
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "diagnostic-5o"))
from image5o import creer, lecteur, verifier

if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("operation", choices=("creer", "verifier"))
    p.add_argument("efi", type=Path)
    p.add_argument("image", type=Path)
    p.add_argument("--octets", type=int, required=True)
    a = p.parse_args()
    with lecteur(a.efi) as f:
        pe = f.read(2 * 1024 * 1024 + 1)
        if len(pe) > 2 * 1024 * 1024:
            raise ValueError("Taille du PE hors de la borne du profil")
    reference = (
        Path(__file__).resolve().parents[2]
        / "docs/validation/diagnostic-uefi-5w/standard.sha256"
    )
    if hashlib.sha256(pe).hexdigest() != reference.read_text().split()[0]:
        raise ValueError("Le PE diffère du profil STANDARD qualifié")
    if a.operation == "creer":
        creer(a.image, a.octets, pe)
    with lecteur(a.image) as f:
        print(json.dumps(verifier(f, a.octets, pe)))
