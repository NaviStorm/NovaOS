"""Remonte une copie de l'image après erreur de fusion et vérifie les lots et la source."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
p=argparse.ArgumentParser();p.add_argument("recepteur",type=Path)
p.add_argument("essai",type=Path);p.add_argument("outil",type=Path)
a=p.parse_args()
essai=a.essai.resolve()
meta=json.loads((essai/"resultats.json").read_text())
out=essai/"reprise";out.mkdir(exist_ok=False)
disk=out/"session.raw"
shutil.copyfile(essai/"session.raw",disk)
sys.path.insert(0,str(a.recepteur.resolve()/"novaOS/tests"))
from cli import Terminal
from journalisation import arret
os.environ.update(NOVA_ARCH=meta["arch"],NOVA_RAM="256",NOVA_VCPUS="2")
ctl=[a.outil.resolve(),disk,str(meta["secteur"])]
def etat():
    return dict(lots=int(subprocess.check_output([*ctl,"etat"])),
                source_sha256=hashlib.sha256(subprocess.check_output([*ctl,"lire"])).hexdigest())
r={"avant":etat()}
with Terminal(meta["arch"],meta["secteur"],disk,"reprise",test=True,journal=out/"reprise.serial") as vm:
    vm.until(b"nsh> ",timeout=90)
    arret(vm,2)
r["apres"]=etat()
(out/"verification.json").write_text(json.dumps(r,indent=2)+"\n")
assert r["apres"]["lots"]==0 and r["avant"]["source_sha256"]==r["apres"]["source_sha256"]==meta["source_sha256"]
