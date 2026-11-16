"""Archive un banc terminé, puis vérifie chaque membre par relecture des deux copies."""
import argparse
import hashlib
import json
import re
import tarfile
from pathlib import Path

def digest(f):
    return hashlib.file_digest(f,"sha256").hexdigest()

def archive(source,dest):
    source=source.resolve(strict=True)
    dest.mkdir(parents=True,exist_ok=False)
    hashes={}
    paths=sorted(p for p in source.iterdir() if p.is_file())
    with tarfile.open(dest/"session.tar.gz","w:gz",compresslevel=6) as out:
        for p in paths:
            with p.open("rb") as f:hashes[p.name]=digest(f)
            out.add(p,arcname=p.name,recursive=False)
    with tarfile.open(dest/"session.tar.gz","r|gz") as copy:
        for member in copy:
            with copy.extractfile(member) as f:
                assert digest(f)==hashes[member.name],member.name
    (dest/"sources.sha256").write_text("".join(f"{s}  {name}\n" for name,s in hashes.items()))
    if (source/"matrice.json").is_file():
        rows=json.loads((source/"matrice.json").read_text())
        for row in rows:
            if "stages" in row:
                raw=bytes.fromhex(row["actual_hex"])
                row["stages"]={k.decode():int(v) for k,v in re.findall(rb"\[pipe ([a-z]+)=(\d+)\]",raw)}
                assert row["stages"] in ({"sort":0,"uniq":0},{"cat":0,"uniq":0})
        (dest/"matrice.json").write_text(json.dumps(rows,ensure_ascii=False,indent=1)+"\n")
        (dest/"corpus.json").write_bytes((source/"corpus.json").read_bytes())
    if (source/"grand.json").is_file():
        (dest/"grand.json").write_bytes((source/"grand.json").read_bytes())
    print(source.name,len(paths),"membres relus",flush=True)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("source",type=Path)
    p.add_argument("dest",type=Path)
    a=p.parse_args()
    archive(a.source,a.dest)
