"""Archiver des fichiers réguliers et relire chaque membre depuis le support final."""
import argparse
import hashlib
import io
from pathlib import Path
import tarfile

def fingerprint(f):
    return hashlib.file_digest(f,"sha256").hexdigest()

def archive(target,files):
    files=sorted(files,key=lambda pair:pair[0])
    assert len(set(name for name,_ in files))==len(files)
    manifest={}
    with tarfile.open(target,"x:xz",preset=4) as tar:
        for name,path in files:
            assert path.is_file() and not path.is_symlink(),path
            with path.open("rb") as stream: manifest[name]=fingerprint(stream)
            tar.add(path,arcname=name,recursive=False)
        raw="".join(f"{sha}  {name}\n" for name,sha in manifest.items()).encode()
        info=tarfile.TarInfo("SHA256SUMS")
        info.size=len(raw)
        tar.addfile(info,io.BytesIO(raw))
    with tarfile.open(target,"r:xz") as tar:
        assert tar.getnames()==[*manifest,"SHA256SUMS"]
        assert tar.extractfile("SHA256SUMS").read()==raw
        for entry in tar:
            if entry.name!="SHA256SUMS":
                assert fingerprint(tar.extractfile(entry))==manifest[entry.name],entry.name
    with target.open("rb") as stream: print(fingerprint(stream),len(files),target.resolve(),flush=True)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("destination",type=Path)
    p.add_argument("sources",type=Path,nargs="+")
    a=p.parse_args()
    files=[]
    for source in a.sources:
        if source.is_dir():
            files.extend((str(path.relative_to(source.parent)),path) for path in source.rglob("*") if path.is_file())
        else: files.append((source.name,source))
    archive(a.destination,files)
if __name__=="__main__":main()
