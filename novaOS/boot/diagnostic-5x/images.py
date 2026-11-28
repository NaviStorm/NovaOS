"""Reconstruire deux fois, écrire avec dd sur fichier neuf et relire le disque entier."""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

NOVA = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(NOVA / "boot/diagnostic-5o"), str(NOVA / "boot/diagnostic-5m")]
from image5o import creer, lecteur, sha, verifier
from image import pe_info

REFERENCE = NOVA / "docs/validation/diagnostic-uefi-5w"
CAPACITES = (68 << 20, (96 << 20) + 512, 1 << 30)


def exercice(dossier, taille, pe):
    with tempfile.TemporaryDirectory(dir=dossier) as espace:
        rep = Path(espace)
        a, b, copie = (rep / n for n in ("a.img", "b.img", "copie.img"))
        for p in (a, b):
            creer(p, taille, pe)
        subprocess.run(["cmp", a, b], check=True)
        with lecteur(a) as source, copie.open("x+b", buffering=0) as cible:
            cible.truncate(taille)
            # Témoin dans le dernier secteur. Même la fin du fichier doit être remplacée.
            os.pwrite(cible.fileno(), bytes([0xa5]) * 512, taille - 512)
            subprocess.run(["dd", "bs=1048576", "conv=notrunc"], stdin=source, stdout=cible,
                           stderr=subprocess.PIPE, check=True)
            os.fsync(cible.fileno())
        subprocess.run(["cmp", a, copie], check=True)
        with lecteur(copie) as f:
            resultat = verifier(f, taille, pe)
            empreinte = sha(f)
        if taille == CAPACITES[0]:
            archive = REFERENCE / "novaos-standard-68mio.img.gz"
            h = hashlib.sha256()
            with gzip.open(archive, "rb") as f:
                for bloc in iter(lambda: f.read(1048576), b""):
                    h.update(bloc)
            if empreinte != h.hexdigest():
                raise ValueError("Écart avec le disque brut archivé en 5W")
        # Épreuve négative sur la seule copie de travail, après les comparaisons.
        with copie.open("r+b") as f:
            f.seek(taille - 512)
            if f.read(8) != b"EFI PART":
                raise ValueError("GPT secondaire introuvable à la fin du support")
            f.seek(taille - 512)
            f.write(b"!")
        with lecteur(copie) as f:
            try:
                verifier(f, taille, pe)
            except ValueError:
                pass
            else:
                raise ValueError("GPT secondaire altérée admise")
        del resultat["alloues"]  # L'allocation physique du fichier dépend du FS de l'hôte.
        return dict(**resultat, sha256=empreinte, deux_generations_identiques=True,
                    dd_identique=True, gpt_alteree_refusee=True)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("efi_reconstruit", type=Path)
    args = p.parse_args()
    archive = gzip.decompress((REFERENCE / "standard.efi.gz").read_bytes())
    with lecteur(args.efi_reconstruit) as f:
        pe = f.read(len(archive) + 1)
    identite = pe_info(pe)
    if pe != archive or identite["sha256"] != (REFERENCE / "standard.sha256").read_text().split()[0]:
        raise ValueError("Le PE reconstruit diffère de la référence 5W")
    with tempfile.TemporaryDirectory(dir=NOVA / "build") as dossier:
        rows = [exercice(dossier, taille, pe) for taille in CAPACITES]
    print(json.dumps(dict(pe_octets=len(pe), pe_sha256=identite["sha256"],
                          authenticode_sha256=identite["authenticode"], images=rows), indent=2))


if __name__ == "__main__":
    main()
