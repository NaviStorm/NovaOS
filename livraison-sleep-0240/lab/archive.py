"""Créer le paquet ou vérifier indépendamment ses membres après décompression."""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

HERE = Path(__file__).resolve().parents[1]
ROOT = HERE.parent
BASE = "425e4e7cc5c03e246efb64d3638376906344bf47"
SOURCE = "5a4864727badcb897397de17e3c701baf7f60cfb"
NAME = "sleep-0240.tar.gz"
MANIFEST = "SHA256SUMS"


def digest(stream):
    return hashlib.file_digest(stream, "sha256").hexdigest()


def sha(path):
    with path.open("rb") as stream:
        return digest(stream)


def verify_media():
    media = sorted((HERE / "preuves").glob("media-*.tar.xz"))
    assert len(media) == 8
    for path in media:
        name = path.name[6:-7]
        profile = name.split("-")[0]
        with gzip.open(HERE / "preuves" / (name + ".json.gz"), "rt") as f:
            proof = json.load(f)
        with tarfile.open(path, "r:xz") as archive:
            assert (
                digest(archive.extractfile(profile + ".iso")) == proof["media"]["iso"]
            )
            assert digest(archive.extractfile("session.raw")) == (
                proof["p9"] if profile == "sdk" else proof["apres"][0]
            )
            if profile == "sdk":
                assert digest(archive.extractfile("p9-initial.raw")) == proof["p9"]


def verify_logs():
    proofs = HERE / "preuves"
    expected = [
        entry.split("  ", 1)
        for entry in (proofs / "journaux.sha256").read_text().splitlines()
    ]
    with tarfile.open(proofs / "journaux.tar.gz", "r:gz") as tar:
        assert tar.getnames() == [name for _, name in expected]
        for h, name in expected:
            assert digest(tar.extractfile(name)) == h, name


def collect_logs():
    proofs = HERE / "preuves"
    source = proofs / "journaux"
    if source.is_dir():
        files = sorted(path for path in source.iterdir() if path.is_file())
        statuses = (source / "regressions.tsv").read_text()
        (proofs / "regressions.tsv").write_text(statuses)
        manifest = "".join(f"{sha(path)}  {path.name}\n" for path in files)
        with tarfile.open(proofs / "journaux.tar.gz", "w:gz") as tar:
            for path in files:
                tar.add(path, arcname=path.name, recursive=False)
        (proofs / "journaux.sha256").write_text(manifest)
    verify_logs()


def make():
    # Un chemin relatif au worktree permet de superposer ce paquet à sa base.
    files = {
        ROOT / p
        for p in subprocess.check_output(
            ["git", "diff", "--name-only", BASE, SOURCE], cwd=ROOT, text=True
        ).splitlines()
    }
    for path in HERE.rglob("*"):
        if not path.is_file():
            continue
        rel = path.relative_to(HERE)
        if rel.parts[:2] == ("preuves", "journaux") or "__pycache__" in rel.parts:
            continue
        if str(rel) not in (NAME, MANIFEST, "SHA256-archive"):
            files.add(path)
    files = sorted(files)
    (HERE / MANIFEST).write_text(
        "".join(f"{sha(p)}  {p.relative_to(ROOT)}\n" for p in files)
    )
    with tempfile.NamedTemporaryFile(dir=os.environ["TMPDIR"], suffix=".tar.gz") as tmp:
        with tarfile.open(fileobj=tmp, mode="w:gz", compresslevel=5) as archive:
            for path in [*files, HERE / MANIFEST]:
                archive.add(path, arcname=str(path.relative_to(ROOT)), recursive=False)
        tmp.flush()
        import shutil

        shutil.copyfile(tmp.name, HERE / NAME)
    (HERE / "SHA256-archive").write_text(f"{sha(HERE / NAME)}  {NAME}\n")


def verify():
    expected = (HERE / "SHA256-archive").read_text().strip().split("  ")
    assert expected == [sha(HERE / NAME), NAME]
    with tarfile.open(HERE / NAME, "r:gz") as archive:
        manifest = str((HERE / MANIFEST).relative_to(ROOT))
        contents = archive.extractfile(manifest).read()
        assert contents == (HERE / MANIFEST).read_bytes()
        entries = [line.split("  ", 1) for line in contents.decode().splitlines()]
        assert archive.getnames() == [p for _, p in entries] + [manifest]
        for h, p in entries:
            assert digest(archive.extractfile(p)) == h == sha(ROOT / p), p
    print(f"{len(entries)} membres et manifeste relus, SHA-256 {expected[0]}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--creer", action="store_true")
    if parser.parse_args().creer:
        collect_logs()
        make()
    verify()
    verify_media()
    verify_logs()
