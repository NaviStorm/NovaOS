"""Retrouve les binaires liés dans le noyau effectivement contenu dans l'ISO QEMU."""
import argparse
import hashlib
import json
from pathlib import Path

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for arg in ("iso","kernel","artifacts"):parser.add_argument(arg,type=Path)
    a=parser.parse_args()
    image=a.iso.read_bytes()
    kernel=a.kernel.read_bytes()
    begin=image.find(kernel)
    if begin<0 or image.find(kernel,begin+1)>=0:raise ValueError("noyau absent ou ambigu")
    def item(offset,raw):
        return {"offset":offset,"taille":len(raw),"sha256":hashlib.sha256(raw).hexdigest()}
    offsets={"noyau":item(begin,kernel)}
    for name in ("nsh.elf","auth-verifier.elf","exec-relay.elf","shell-signer.elf",
                 "storage-media.bin","service_manager.bin","storage-managed.bin"):
        raw=(a.artifacts/name).read_bytes()
        i=kernel.find(raw)
        if not raw or i<0:raise ValueError(("composant absent",name))
        copy=1
        while i>=0:
            offsets[name if copy==1 else f"{name}#{copy}"]=item(begin+i,raw)
            i=kernel.find(raw,i+len(raw));copy+=1
    with a.iso.with_suffix(".composants.json").open("x") as f:json.dump(offsets,f,indent=2)
if __name__=="__main__":main()
