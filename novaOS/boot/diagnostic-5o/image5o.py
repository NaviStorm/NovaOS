"""Fabrication et contrôle de fichiers image x86_64. Aucune destination bloc."""

import argparse
import contextlib
import errno
import gzip
import hashlib
import json
import mmap
import os
from pathlib import Path
import stat
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "boot/diagnostic-5n"))
from support import ESP, LOG, MIN_MIO, MIO, S, gpt, notices, require, volume
from verifier5n import fat_files, geometry

PREUVES = ROOT / "docs/validation/diagnostic-uefi-5o"
BLOC = MIO


def chargeur():
    pe = gzip.decompress((PREUVES / "diagnostic-x86_64.efi.gz").read_bytes())
    require(
        hashlib.sha256(pe).hexdigest()
        == (PREUVES / "efi.sha256").read_text().split()[0],
        "Identité du PE 5O",
    )
    return pe


def secteurs(octets):
    require(
        isinstance(octets, int)
        and MIN_MIO * MIO <= octets < (1 << 32) * S
        and octets % S == 0,
        "Capacité exacte attendue en octets, secteurs de 512 octets",
    )
    return octets // S


def contenu(capacite, pe):
    blocks = gpt(secteurs(capacite))
    esp, _ = volume(ESP, "NOVAOS", pe)
    logs, _ = volume(LOG, "NOVALOGS")
    blocks.update(esp)
    blocks.update(logs)
    return blocks


@contextlib.contextmanager
def lecteur(path):
    initial = path.lstat()
    if not stat.S_ISREG(initial.st_mode):
        raise OSError("La source doit être un fichier ordinaire sans lien symbolique")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        opened = os.fstat(fd)
        require(
            stat.S_ISREG(opened.st_mode) and os.path.samestat(initial, opened),
            "Identité du fichier image modifiée avant ouverture",
        )
        with os.fdopen(fd, "rb", buffering=0, closefd=False) as f:
            yield f
    finally:
        os.close(fd)


def creer(path, capacite, pe):
    blocks = contenu(capacite, pe)
    with path.open("xb", buffering=0) as f:
        f.truncate(capacite)
        for lba, data in sorted(blocks.items()):
            f.seek(lba * S)
            require(f.write(data) == len(data), "Écriture courte de l’image")
        os.fsync(f.fileno())


def etendues(fd, size):
    """Les trous déclarés par le système de fichiers se lisent comme des zéros."""
    if not hasattr(os, "SEEK_DATA") or not hasattr(os, "SEEK_HOLE"):
        yield 0, size
        return
    pos = 0
    while pos < size:
        try:
            first = os.lseek(fd, pos, os.SEEK_DATA)
        except OSError as exc:
            if exc.errno == errno.ENXIO:
                return
            if exc.errno not in (errno.EINVAL, errno.ENOTSUP):
                raise
            yield pos, size
            return
        if first == size:
            return
        try:
            last = os.lseek(fd, first, os.SEEK_HOLE)
        except OSError as exc:
            if exc.errno not in (errno.ENXIO, errno.EINVAL, errno.ENOTSUP):
                raise
            last = size
        require(pos <= first < last <= size, "Étendue de fichier invalide")
        yield first, last
        pos = last


def verifier(f, capacite, pe):
    size = os.fstat(f.fileno()).st_size
    require(size == capacite, "Capacité différente de la cible déclarée")
    blocks = contenu(size, pe)
    expected = [(l * S, bytes(data)) for l, data in sorted(blocks.items())]
    # Les plages non vides attendues doivent exister même si le FS rapporte un trou.
    for pos, data in expected:
        require(
            os.pread(f.fileno(), len(data), pos) == data,
            "GPT, FAT ou contenu attendu altéré",
        )
    idx = 0
    for first, last in etendues(f.fileno(), size):
        for pos in range(first, last, BLOC):
            length = min(BLOC, last - pos)
            canonical = bytearray(length)
            while (
                idx < len(expected) and expected[idx][0] + len(expected[idx][1]) <= pos
            ):
                idx += 1
            for off, data in expected[idx:]:
                if off >= pos + length:
                    break
                lo, hi = max(pos, off), min(pos + length, off + len(data))
                canonical[lo - pos : hi - pos] = data[lo - off : hi - off]
            require(
                os.pread(f.fileno(), length, pos) == canonical,
                "Zone libre ou journal initial altéré",
            )
    with mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as disk:
        parts = geometry(disk)
        e = fat_files(disk, parts["NOVAOS"], "NOVAOS")
        logs = fat_files(disk, parts["NOVALOGS"], "NOVALOGS")
        require(
            e == {"EFI/BOOT/BOOTX64.EFI": pe, "LICENCES.TXT": notices()}
            and len(logs) == 24
            and all(not any(v) for v in logs.values()),
            "Relecture FAT32 indépendante",
        )
    return dict(
        octets=size,
        secondaire_lba=size // S - 1,
        partitions=parts,
        fichiers_prealloues=len(logs),
        alloues=os.fstat(f.fileno()).st_blocks * 512,
    )


def copier(source, cible, capacite, pe):
    with lecteur(source) as f:
        verifier(f, capacite, pe)
        with cible.open("xb", buffering=0) as d:
            d.truncate(capacite)
            for first, last in etendues(f.fileno(), capacite):
                for pos in range(first, last, BLOC):
                    n = min(BLOC, last - pos)
                    b = os.pread(f.fileno(), n, pos)
                    require(len(b) == n, "Lecture courte")
                    if any(b):
                        require(os.pwrite(d.fileno(), b, pos) == n, "Copie courte")
            os.fsync(d.fileno())
    with lecteur(cible) as d:
        return verifier(d, capacite, pe)


def sha(f):
    h = hashlib.sha256()
    f.seek(0)
    for data in iter(lambda: f.read(BLOC), b""):
        h.update(data)
    return h.hexdigest()


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("operation", choices=["creer", "verifier", "copier"])
    ap.add_argument("image", type=Path)
    ap.add_argument("--octets", required=True, type=int)
    ap.add_argument("--sortie", type=Path)
    a = ap.parse_args()
    pe = chargeur()
    if a.operation == "copier":
        require(a.sortie is not None, "Destination neuve attendue")
        print(json.dumps(copier(a.image, a.sortie, a.octets, pe)))
    else:
        if a.operation == "creer":
            creer(a.image, a.octets, pe)
        with lecteur(a.image) as f:
            print(json.dumps(verifier(f, a.octets, pe)))
