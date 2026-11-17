"""Relit tous les membres sans extraction et vérifie le manifeste de l'archive."""
import hashlib
from pathlib import Path
import sys
import tarfile

def verify(archive, manifest):
    expected = {}
    for line in manifest.read_text().splitlines():
        digest, name = line.split("  ", 1)
        if name in expected:
            raise ValueError(name)
        expected[name] = digest
    with tarfile.open(archive, "r|xz") as bundle:
        for member in bundle:
            if not member.isfile():
                raise ValueError(member.name)
            with bundle.extractfile(member) as source:
                actual = hashlib.file_digest(source,"sha256").hexdigest()
            if actual != expected.pop(member.name):
                raise ValueError(member.name)
    if expected:
        raise ValueError(list(expected))
    print(f"{archive.name} vérifiée, {len(manifest.read_text().splitlines())} fichiers")

if __name__ == "__main__":
    base = Path(__file__).resolve().parents[1]
    verify(base/"qualification.tar.xz",base/"qualification.sha256")
