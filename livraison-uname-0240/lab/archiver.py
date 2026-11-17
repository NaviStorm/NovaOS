"""Archiver les pièces de la livraison et vérifier les octets de chaque membre décompressé."""
import argparse
import hashlib
import io
from pathlib import Path
import tarfile

HERE = Path(__file__).resolve().parents[1]
MANIFEST = "SHA256SUMS"

def digest(source):
    return hashlib.file_digest(source, "sha256").hexdigest()

def verify(path):
    with tarfile.open(path,"r:gz") as archive:
        members=archive.getmembers()
        names=[m.name for m in members]
        if len(names)!=len(set(names)) or any(not m.isfile() for m in members):
            raise ValueError("Membres dupliqués ou de type inattendu")
        lines=archive.extractfile(MANIFEST).read().decode().splitlines()
        expected={}
        for line in lines:
            checksum,name=line.split("  ",1)
            if name in expected: raise ValueError(name)
            expected[name]=checksum
        if set(expected)!=(set(names)-{MANIFEST}): raise ValueError("Inventaire différent")
        for name,checksum in expected.items():
            with archive.extractfile(name) as stream:
                if digest(stream)!=checksum: raise ValueError(name)
    return len(expected)

def create(path):
    if path.resolve().is_relative_to(HERE):
        raise ValueError("Placer l’archive hors du dossier de livraison")
    files=sorted(p for p in HERE.rglob("*") if p.is_file() and "__pycache__" not in p.parts)
    records=[]
    path.parent.mkdir(parents=True,exist_ok=True)
    with tarfile.open(path,"x:gz",compresslevel=2) as archive:
        for p in files:
            name=p.relative_to(HERE).as_posix()
            if name==MANIFEST: continue
            with p.open("rb") as src:
                records.append(f"{digest(src)}  {name}\n")
                src.seek(0)
                archive.addfile(archive.gettarinfo(str(p),arcname=name),src)
        data="".join(records).encode()
        entry=tarfile.TarInfo(MANIFEST);entry.size=len(data)
        archive.addfile(entry,io.BytesIO(data))

if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action",choices=["creer","verifier"]);parser.add_argument("archive",type=Path)
    args=parser.parse_args()
    if args.action=="creer":create(args.archive)
    print(verify(args.archive),"fichiers décompressés et vérifiés")
    with args.archive.open("rb") as src: print(digest(src),args.archive)
