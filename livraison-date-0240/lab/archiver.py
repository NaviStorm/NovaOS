"""Archive autonome des essais. Vérification de chaque fichier après extraction."""
import argparse
import hashlib
import os
from pathlib import Path
import tarfile
import tempfile

HERE = Path(__file__).resolve().parents[1]
ARCHIVE = "date-arc027.tar.gz"
MANIFEST = "MANIFESTE.sha256"

def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def verify(root):
    rows = (root / MANIFEST).read_text().splitlines()
    names = set()
    for row in rows:
        digest, name = row.split("  ", 1)
        path = root / name
        if name in names or not path.is_file() or sha(path) != digest:
            raise ValueError(name)
        names.add(name)
    actual = {p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file()}
    if actual != names | {MANIFEST}:
        raise ValueError("Inventaire extrait différent du manifeste")
    return len(names)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verifier", action="store_true")
    args = parser.parse_args()
    tmp = Path(os.environ.get("NOVAOS_TMPDIR", HERE.parent.parent/"tmp"))
    target = HERE / ARCHIVE
    sidecar = HERE / (ARCHIVE + ".sha256")
    if not args.verifier:
        paths = sorted(p.relative_to(HERE).as_posix() for p in HERE.rglob("*")
                       if p.is_file() and p.name not in {MANIFEST,ARCHIVE,sidecar.name,".DS_Store"}
                       and "__pycache__" not in p.parts and p.suffix != ".partial")
        (HERE / MANIFEST).write_text("".join(f"{sha(HERE/p)}  {p}\n" for p in paths))
        with tempfile.NamedTemporaryFile(dir=HERE, suffix=".partial", delete=False) as candidate:
            partial = Path(candidate.name)
        try:
            with tarfile.open(partial, mode="w:gz", compresslevel=3) as archive:
                for name in [MANIFEST,*paths]:
                    archive.add(HERE/name, arcname=name,recursive=False)
            os.replace(partial,target)
        finally:
            partial.unlink(missing_ok=True)
        sidecar.write_text(f"{sha(target)}  {ARCHIVE}\n")
    if sha(target) != sidecar.read_text().split()[0]:
        raise ValueError("SHA-256 de l’archive")
    with tempfile.TemporaryDirectory(dir=tmp,prefix="date-verif-") as folder:
        with tarfile.open(target) as archive:
            archive.extractall(folder,filter="data")
        print(f"{verify(Path(folder))} fichiers relus après décompression. SHA-256 {sha(target)}")

if __name__ == "__main__":
    main()
