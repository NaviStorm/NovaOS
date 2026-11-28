"""Essai hôte sur un fichier temporaire privé. Aucun argument de destination."""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5o"))
from image5o import lecteur, sha, verifier

FENETRE = 1024 * 1024
BLOC = 4096


def synchroniser(source, cible, taille):
    ecrits = 0
    for pos in range(0, taille, FENETRE):
        n = min(FENETRE, taille - pos)
        a = os.pread(source.fileno(), n, pos)
        b = os.pread(cible.fileno(), n, pos)
        if len(a) != n or len(b) != n:
            raise OSError("Lecture partielle")
        if a == b:
            continue
        for k in range(0, n, BLOC):
            bloc = a[k:k + BLOC]
            if bloc != b[k:k + BLOC]:
                if os.pwrite(cible.fileno(), bloc, pos + k) != len(bloc):
                    raise OSError("Écriture partielle")
                ecrits += len(bloc)
    os.fsync(cible.fileno())
    return ecrits


def simuler(image, octets, attendu):
    archive = NOVA / "docs/validation/diagnostic-uefi-5w"
    pe = gzip.decompress((archive / "standard.efi.gz").read_bytes())
    if hashlib.sha256(pe).hexdigest() != (archive / "standard.sha256").read_text().split()[0]:
        raise ValueError("PE de référence incohérent")
    with lecteur(image) as src:
        geo = verifier(src, octets, pe)
        if sha(src) != attendu:
            raise ValueError("Empreinte du disque source incohérente")
        with tempfile.TemporaryFile(dir=NOVA / "build", buffering=0) as dst:
            dst.truncate(octets)
            # Préfixe déjà copié et donnée résiduelle hors partitions, avant la GPT secondaire.
            ancien = octets - 65536
            if os.pread(src.fileno(), BLOC, ancien) != bytes(BLOC):
                raise ValueError("Le témoin exige une plage nulle dans la référence")
            prefixe = os.pread(src.fileno(), BLOC, 0)
            if os.pwrite(dst.fileno(), prefixe, 0) != BLOC or os.pwrite(dst.fileno(), b"!", ancien) != 1:
                raise OSError("Initialisation du témoin incomplète")
            ecrits = synchroniser(src, dst, octets)
            if not 0 < ecrits < octets or sha(dst) != attendu:
                raise ValueError("Copie différentielle incohérente")
            verifier(dst, octets, pe)
            suivants = synchroniser(src, dst, octets)
            if suivants or sha(dst) != attendu:
                raise ValueError("La seconde passe doit être identique sans réécriture")
    return dict(octets=octets, sha256=attendu, gpt_secondaire_lba=geo["secondaire_lba"],
                octets_ecrits=ecrits, octets_ecrits_seconde_passe=suivants,
                bloc_max_octets=BLOC)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("image", type=Path)
    p.add_argument("--octets", type=int, required=True)
    p.add_argument("--sha256", required=True)
    a = p.parse_args()
    print(json.dumps(simuler(a.image, a.octets, a.sha256), indent=2))
