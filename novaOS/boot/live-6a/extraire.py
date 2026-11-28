"""Restituer le fichier disque 6A dans un répertoire de travail ordinaire."""
import argparse
import gzip
from hashlib import sha256
import os
from pathlib import Path
import tempfile

TAILLE = 4005560320
EMPREINTE = "9945b4334170495d9a9836f1f8c9224c8293600b33445fc8f7c4d799ba26961a"


def extraire(archive, destination):
    zeros = bytes(1 << 20)
    h, total = sha256(), 0
    if destination.exists() or destination.is_symlink():
        raise FileExistsError(destination)
    with tempfile.NamedTemporaryFile(dir=destination.parent) as sortie:
        with gzip.open(archive, "rb") as src:
            while bloc := src.read(len(zeros)):
                total += len(bloc)
                if total > TAILLE:
                    raise ValueError("Archive trop grande")
                h.update(bloc)
                if bloc == zeros:
                    sortie.seek(len(bloc), 1)
                else:
                    sortie.write(bloc)
        if total != TAILLE or h.hexdigest() != EMPREINTE:
            raise ValueError("Taille ou SHA-256 de l’image incorrect")
        sortie.truncate(total)
        sortie.flush()
        os.fsync(sortie.fileno())
        # Publication exclusive sur le même volume, après contrôle de tous les octets.
        os.link(sortie.name, destination)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=Path)
    p.add_argument("destination", type=Path)
    a = p.parse_args()
    extraire(a.archive, a.destination)
    print(EMPREINTE, a.destination.name)
