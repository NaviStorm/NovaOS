"""Archive les pièces nommées, puis compare chaque membre relu à son original."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile

DELIVERY = Path(__file__).resolve().parents[1]

def sha(path):
    with path.open("rb") as src:
        return hashlib.file_digest(src,"sha256").hexdigest()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("temporaires",type=Path)
    parser.add_argument("archive",type=Path)
    args=parser.parse_args()
    inputs={}
    for arch in ("aarch64","x86_64"):
        for sector in (512,4096):
            tag=f"{arch}-{sector}"
            folder=args.temporaires/("final-"+tag)
            for name in ("genese.raw","session.raw","nsh.iso","labo.iso","elf.iso","sdk-kernel.elf",
                         "nsh.vm.json","labo.vm.json","nsh.serial","labo.serial","elf.serial","elf.json","resultats.json"):
                inputs[f"qemu/{tag}/{name}"]=folder/name
            for name in ("final-"+tag,"elf-"+tag):
                inputs[f"construction/{name}.log"]=args.temporaires/(name+".log")
        for name in ("final-elf-"+arch,"final-kernel-"+arch,"final-labo-"+arch,arch+"-nsh-scripts-test",arch+"-cli-p9-test"):
            inputs[f"construction/{name}.log"]=args.temporaires/(name+".log")
    inputs["construction/registre.log"]=args.temporaires/"registre.log"
    for folder in ("bin","preuves"):
        for p in sorted((DELIVERY/folder).rglob("*")):
            if p.is_file():inputs[str(p.relative_to(DELIVERY))]=p
    manifest={name:sha(p) for name,p in inputs.items()}
    encoded=json.dumps(manifest,indent=2,sort_keys=True).encode()+b"\n"
    with tarfile.open(args.archive,"x:xz",preset=3) as arc:
        for name,p in inputs.items():
            arc.add(p,arcname=name,recursive=False)
        info=tarfile.TarInfo("MANIFESTE.json")
        info.size=len(encoded)
        arc.addfile(info,io.BytesIO(encoded))
    with tarfile.open(args.archive,"r:xz") as arc:
        seen=set()
        for member in arc:
            with arc.extractfile(member) as f:
                if member.name=="MANIFESTE.json":
                    assert f.read()==encoded
                else:
                    assert member.name not in seen
                    assert hashlib.file_digest(f,"sha256").hexdigest()==manifest[member.name],member.name
                    seen.add(member.name)
        assert seen==set(inputs)
    print(f"{len(inputs)} fichiers relus et vérifiés. SHA-256 {sha(args.archive)}")

if __name__=="__main__":main()
