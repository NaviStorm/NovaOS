"""Relire les fichiers distribués et tous les membres de chaque archive de preuves."""

import hashlib
from pathlib import Path
import tarfile

HERE = Path(__file__).resolve().parent


def digest(stream):
    return hashlib.file_digest(stream, "sha256").hexdigest()


def inventory(text):
    pairs = [line.split("  ", 1) for line in text.splitlines()]
    result = {name: value for value, name in pairs}
    if len(result) != len(pairs):
        raise ValueError("Nom dupliqué dans le manifeste")
    return result


def verify(path):
    with tarfile.open(path, "r:xz") as archive:
        files = {item.name for item in archive.getmembers() if item.isfile()}
        expected = inventory(archive.extractfile("SHA256SUMS").read().decode())
        if files != {*expected, "SHA256SUMS"}:
            raise ValueError(f"Inventaire incomplet dans {path}")
        for name, value in expected.items():
            with archive.extractfile(name) as src:
                if digest(src) != value:
                    raise ValueError(f"{path} / {name}")
    print(f"{path.name} — {len(expected)} fichiers relus")


if __name__ == "__main__":
    entries = inventory((HERE / "SHA256SUMS").read_text())
    for name, value in entries.items():
        with (HERE / name).open("rb") as src:
            if digest(src) != value:
                raise ValueError(name)
    for path in sorted((HERE / "preuves").glob("*.tar.xz")):
        verify(path)
    print(f"{len(entries)} empreintes de livraison vérifiées")
