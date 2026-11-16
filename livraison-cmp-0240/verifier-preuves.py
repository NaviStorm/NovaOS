"""Relire tous les membres des archives de qualification et le manifeste de livraison."""
import hashlib
import json
import tarfile
from pathlib import Path

here = Path(__file__).resolve().parent
for path in sorted((here/"preuves").glob("*.tar.xz")):
    with tarfile.open(path) as archive:
        expected = json.load(archive.extractfile("sha256.json"))
        files = [m for m in archive if m.isfile() and m.name != "sha256.json"]
        assert len(files) == len(expected) and {m.name for m in files} == set(expected), path
        for member in files:
            with archive.extractfile(member) as data:
                assert hashlib.file_digest(data,"sha256").hexdigest() == expected[member.name], (path,member.name)
    print(path.name, len(expected), "fichiers vérifiés")
for line in (here/"SHA256SUMS").read_text().splitlines():
    checksum, name = line.split("  ",1)
    with (here/name).open("rb") as data:
        assert hashlib.file_digest(data,"sha256").hexdigest() == checksum, name
print("Manifeste de livraison vérifié")
