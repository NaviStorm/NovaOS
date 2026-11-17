"""Archiver les originaux d'une campagne et comparer chaque membre relu à son original."""

import argparse
import hashlib
import io
from pathlib import Path
import tarfile


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def pack(source, dest):
    source = source.resolve(strict=True)
    files = {
        str(p.relative_to(source)): p for p in sorted(source.rglob("*")) if p.is_file()
    }
    if not files or "SHA256SUMS" in files:
        raise ValueError("Répertoire de preuves vide ou déjà archivé")
    hashes = {name: digest(p) for name, p in files.items()}
    manifest = "".join(f"{value}  {name}\n" for name, value in hashes.items()).encode()
    with tarfile.open(dest, "x:xz", preset=4) as arc:
        for name, p in files.items():
            arc.add(p, arcname=name, recursive=False)
        info = tarfile.TarInfo("SHA256SUMS")
        info.size = len(manifest)
        arc.addfile(info, io.BytesIO(manifest))
    with tarfile.open(dest, "r:xz") as arc:
        if set(arc.getnames()) != {*files, "SHA256SUMS"}:
            raise ValueError("Différence d'inventaire")
        assert arc.extractfile("SHA256SUMS").read() == manifest
        # Sequential reading avoids repeated decompression of earlier members.
        for name, original in files.items():
            if hashlib.file_digest(
                arc.extractfile(name), "sha256"
            ).hexdigest() != digest(original):
                raise ValueError(f"Relecture différente : {name}")
    print(f"{dest.name} — {len(files)} originaux identiques — {digest(dest)}")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("source", type=Path)
    p.add_argument("archive", type=Path)
    a = p.parse_args()
    pack(a.source, a.archive)
