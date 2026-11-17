"""Relire les octets décompressés et imposer l'ensemble exact du manifeste."""
import hashlib
import tarfile
from pathlib import Path

delivery=Path(__file__).resolve().parents[1]
expected={}
for line in (delivery/"qualification.sha256").read_text().splitlines():
    digest,name=line.split("  ",1)
    assert name not in expected
    expected[name]=digest
with tarfile.open(delivery/"qualification.tar.xz","r|xz") as archive:
    for member in archive:
        assert member.isfile() and member.name in expected,member.name
        with archive.extractfile(member) as f:
            assert hashlib.file_digest(f,"sha256").hexdigest()==expected.pop(member.name),member.name
assert not expected,expected.keys()
print("Archive et manifeste conformes.")
