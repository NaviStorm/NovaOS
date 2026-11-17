"""Créer ou relire le manifeste des membres décompressés d’une archive de recette dd."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
PROFILES = ("aarch64-512", "aarch64-4096", "x86_64-512", "x86_64-4096")


def digest(stream):
    return hashlib.file_digest(stream, "sha256").hexdigest()


def verify(archive):
    actual = {}
    manifest = None
    with tarfile.open(archive, "r:*") as tar:
        for entry in tar:
            if (
                not entry.isfile()
                or Path(entry.name).is_absolute()
                or ".." in Path(entry.name).parts
            ):
                raise ValueError(entry.name)
            with tar.extractfile(entry) as reader:
                if entry.name == "MANIFESTE.json":
                    if manifest is not None:
                        raise ValueError("manifeste dupliqué")
                    manifest = json.load(reader)
                else:
                    if entry.name in actual:
                        raise ValueError(entry.name)
                    actual[entry.name] = digest(reader)
    if actual != manifest:
        raise ValueError("Empreintes des membres divergentes")
    with archive.open("rb") as f:
        print(
            f"{len(actual)} membres décompressés vérifiés, archive SHA-256 {digest(f)}"
        )


def create(archive, profiles):
    inputs = {}
    for profile in PROFILES:
        folder = profiles / profile
        assert (folder / "resultats.json").is_file(), profile
        for p in folder.iterdir():
            if p.is_file():
                inputs[f"qemu/{profile}/{p.name}"] = p
    for name in (
        "options.json.gz",
        "options.flux.json",
        "performances.json",
        "catalogue.txt",
        "registre-abi.txt",
        "environnement.txt",
        "matrice-macos.tsv",
        "matrice-elf.tsv",
        "regressions",
        "gnu-livraison",
        "construction",
    ):
        path = HERE / "preuves" / name
        for p in path.rglob("*") if path.is_dir() else [path]:
            if p.is_file():
                inputs[str(p.relative_to(HERE))] = p
    for folder in (HERE / "bin", HERE / "licences", ROOT / "components/dd"):
        for p in folder.iterdir():
            if p.is_file():
                inputs[str(p.relative_to(ROOT))] = p
    for p in HERE.iterdir():
        if p.is_file() and p.suffix in (".rs", ".py", ".sh", ".md"):
            inputs[p.name] = p
    manifest = {}
    with tarfile.open(archive, "x:xz", preset=5) as tar:
        for name, p in sorted(inputs.items()):
            with p.open("rb") as source:
                manifest[name] = digest(source)
            tar.add(p, arcname=name, recursive=False)
        data = json.dumps(manifest, indent=2, sort_keys=True).encode() + b"\n"
        info = tarfile.TarInfo("MANIFESTE.json")
        info.size = len(data)
        tar.addfile(info, io.BytesIO(data))
    verify(archive)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--creer", type=Path, metavar="PROFILS")
    args = parser.parse_args()
    if args.creer:
        create(args.archive, args.creer)
    else:
        verify(args.archive)
