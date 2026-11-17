"""Archiver la livraison et les vingt amorçages, puis relire tous les membres."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile

HERE=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("campagne",type=Path)
parser.add_argument("archive",type=Path)
args=parser.parse_args()

def sha(stream):
    return hashlib.file_digest(stream,"sha256").hexdigest()

files={}
for name in ["bin","preuves","lab"]:
    for p in (HERE/name).rglob("*"):
        if p.is_file() and not p.is_symlink() and "__pycache__" not in p.parts and p.name!="archive.sha256":
            files["livraison/"+str(p.relative_to(HERE))]=p
for p in [*HERE.glob("*.sh"),*HERE.glob("*.md")]:
    files["livraison/"+p.name]=p
component=HERE.parent/"components/mktemp"
for p in [*component.glob("Cargo.*"),component/"VERSION",*sorted((component/"src").glob("*.rs"))]:
    files["components/mktemp/"+str(p.relative_to(component))]=p
for arch in ["aarch64","x86_64"]:
    for sector in [512,4096]:
        for test in ["nsh","elf","prive","commande","duo"]:
            key=f"{test}-{arch}-{sector}"
            work=args.campagne/key
            result=json.loads((work/"resultats.json").read_bytes())
            assert (result["arch"],result["secteur"])==(arch,sector)
            for p in work.iterdir():
                if p.is_file():files[key+"/"+p.name]=p
with args.archive.open("xb") as output,tarfile.open(fileobj=output,mode="w:gz",compresslevel=3) as archive:
    sums={}
    for name,path in sorted(files.items()):
        with path.open("rb") as f:sums[name]=sha(f)
        archive.add(path,arcname=name,recursive=False)
    manifest="".join(f"{digest}  {name}\n" for name,digest in sums.items()).encode()
    info=tarfile.TarInfo("SHA256SUMS")
    info.size=len(manifest)
    archive.addfile(info,io.BytesIO(manifest))

with tarfile.open(args.archive,mode="r|gz") as archive:
    observed=set()
    for member in archive:
        assert member.isfile() and member.name not in observed
        observed.add(member.name)
        stream=archive.extractfile(member)
        if member.name=="SHA256SUMS":assert stream.read()==manifest
        else:assert sha(stream)==sums[member.name],member.name
    assert observed==set(sums)|{"SHA256SUMS"}
with args.archive.open("rb") as f:print(sha(f),args.archive,sep="  ")
print(len(sums),"fichiers relus après décompression")
