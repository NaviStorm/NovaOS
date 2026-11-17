"""Relire le paquet entier et vérifier chaque média vis-à-vis du démarrage mesuré."""
import argparse
import gzip
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import tarfile
import tempfile

HERE = Path(__file__).resolve().parents[1]
ROOT = HERE.parent
ARCHIVE = HERE / "seq-0240.tar.gz"
MANIFEST = HERE / "SHA256SUMS"
BASE = "425e4e7cc5c03e246efb64d3638376906344bf47"
SOURCE = "54895020035b7a082e20b9b602cc5682d42bd41b"


def digest(stream):
    return hashlib.file_digest(stream, "sha256").hexdigest()


def file_sha(path):
    with path.open("rb") as data:
        return digest(data)


def verify_media():
    for media in sorted((HERE / "preuves").glob("media-*.tar.xz")):
        name = media.name.removeprefix("media-").removesuffix(".tar.xz")
        profile = name.split("-")[0]
        with gzip.open(HERE / "preuves" / (name + ".json.gz"), "rt") as f:
            result = json.load(f)
        with tarfile.open(media, "r:xz") as tar:
            assert digest(tar.extractfile(profile + ".iso")) == result["media"]["iso"]
            if profile == "sdk":
                for path in ("p9-initial.raw", "session.raw"):
                    assert digest(tar.extractfile(path)) == result["p9"]
            else:
                assert digest(tar.extractfile("session.raw")) == result["apres"][0]


def members():
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", BASE, SOURCE], cwd=ROOT, text=True
    ).splitlines()
    files = [ROOT / rel for rel in changed]
    files.extend(
        path for path in HERE.rglob("*")
        if path.is_file() and path not in (ARCHIVE, MANIFEST) and "__pycache__" not in path.parts
    )
    return sorted(files)


def verify_archive(expected):
    with tarfile.open(ARCHIVE, "r:gz") as archive:
        names = [str(path.relative_to(ROOT)) for path in expected]
        assert archive.getnames() == names
        for path, name in zip(expected, names):
            assert digest(archive.extractfile(name)) == file_sha(path), name


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--creer", action="store_true")
    args = parser.parse_args()
    verify_media()
    expected = members()
    if args.creer:
        with tempfile.NamedTemporaryFile(dir=Path(os.environ["TMPDIR"]), suffix=".tar.gz") as tmp:
            with tarfile.open(fileobj=tmp, mode="w:gz", compresslevel=6) as tar:
                for path in expected:
                    tar.add(path, arcname=str(path.relative_to(ROOT)), recursive=False)
            tmp.flush()
            shutil.copyfile(tmp.name, ARCHIVE)
    verify_archive(expected)
    lines = [f"{file_sha(p)}  {p.relative_to(HERE)}" for p in sorted([p for p in expected if p.is_relative_to(HERE)] + [ARCHIVE])]
    checksum = "\n".join(lines) + "\n"
    if args.creer:
        MANIFEST.write_text(checksum)
    assert MANIFEST.read_text() == checksum
    print(f"{len(expected)} membres relus. SHA-256 {file_sha(ARCHIVE)}")


if __name__ == "__main__":
    main()
