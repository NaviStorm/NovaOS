"""Assemble ou vérifie l'archive source autonome nova-system."""
import argparse
import gzip
import hashlib
import io
from pathlib import Path
import tarfile
import tomllib


DIST = Path(__file__).resolve().parents[1]
OS = DIST.parent / "novaOS"
LIB = OS / "libs/nova-system"
EXAMPLE = OS / "tests/external-threads"


def assemble():
    version = tomllib.loads((LIB / "Cargo.toml").read_text())["package"]["version"]
    name = f"nova-system-{version}"
    files = {
        "LICENSE": OS / "LICENSE",
        "README.md": OS / "docs/sdk-rust-v0.20.0.md",
        "build.py": LIB / "build.py",
        "libs/nova-system/Cargo.toml": LIB / "Cargo.toml",
        "link/novaos.ld": OS / "user/novafs/service.ld",
    }
    for path in sorted((LIB / "src").glob("*.rs")):
        files[str(path.relative_to(OS))] = path
    for shared in ("abi/native.rs", "abi/files.rs", "abi/bulk_call.rs",
                   "user/gate.rs", "user/storage/fileapi.rs", "user/storage/client_io.rs"):
        files[shared] = OS / shared
    for part in ("Cargo.toml", "Cargo.lock", "main.rs", "checks.rs", "fp_aarch64.S", "fp_x86_64.S"):
        files[f"examples/threads/{part}"] = EXAMPLE / part

    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for member, path in sorted(files.items()):
            content = path.read_bytes()
            info = tarfile.TarInfo(f"{name}/{member}")
            info.size = len(content)
            info.mode = 0o644
            archive.addfile(info, io.BytesIO(content))
    return name, raw.getvalue()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="vérifier les deux fichiers distribués")
    args = parser.parse_args()
    name, contents = assemble()
    archive = DIST / "sdk" / f"{name}.tar.gz"
    checksum = archive.with_name(archive.name + ".sha256")
    if args.check:
        data = archive.read_bytes()
        if gzip.decompress(data) != contents:
            parser.exit(1, "Le SDK distribué diffère des sources.\n")
    else:
        data = gzip.compress(contents, compresslevel=9, mtime=0)
        archive.parent.mkdir(exist_ok=True)
        archive.write_bytes(data)
    expected = f"{hashlib.sha256(data).hexdigest()}  {archive.name}\n"
    if args.check:
        if checksum.read_text() != expected:
            parser.exit(1, "L’empreinte du SDK distribué est incorrecte.\n")
    else:
        checksum.write_text(expected)
    print(archive)


if __name__ == "__main__":
    main()
