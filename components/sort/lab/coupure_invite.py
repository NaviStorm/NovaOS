"""Arrêt forcé de QEMU pendant un tri, remontage P9 et dénombrement des lots privés."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
p=argparse.ArgumentParser()
p.add_argument("recepteur",type=Path);p.add_argument("arch",choices=["aarch64","x86_64"])
p.add_argument("sector",type=int,choices=[512,4096]);p.add_argument("out",type=Path)
p.add_argument("outil",type=Path);p.add_argument("--parcours",type=int,default=45)
a=p.parse_args()
out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
sys.path.insert(0,str(a.recepteur.resolve()/"novaOS/tests"))
from cli import Terminal
from pipe_natif import fixture
from journalisation import arret
os.environ.update(NOVA_ARCH=a.arch,NOVA_RAM="256",NOVA_VCPUS="2")
source=b"".join(f"{i%64:02x}".encode().ljust(31,b"q")+b"\n" for i in range(8192))
disk,_,_=fixture(out,a.sector,{"donnees":source})
control=[a.outil.resolve(),disk,str(a.sector)]
def count():
    return int(subprocess.check_output([*control,"etat"]))
report={"arch":a.arch,"secteur":a.sector,"lignes":8192,"octets":len(source),
        "source_sha256":hashlib.sha256(source).hexdigest(),"initial":count(),
        "point_arret":a.parcours}
with Terminal(a.arch,a.sector,disk,"coupure",test=True,journal=out/"coupure.serial") as vm:
    vm.until(b"nsh> ")
    vm.vm.stdin.write(b"sort donnees | uniq -c\n")
    vm.until(b"PIPE admission image=18 ")
    vm.until(f"BLOC parcours i={a.parcours} ".encode(),timeout=90)
    vm.vm.kill()
report["apres_coupure"]=count()
with Terminal(a.arch,a.sector,disk,"reprise",test=True,journal=out/"reprise.serial") as vm:
    vm.until(b"nsh> ",timeout=90)
    arret(vm,2)
report["apres_reprise"]=count()
report["source_intacte"]=subprocess.check_output([*control,"lire"])==source
(out/"resultats.json").write_text(json.dumps(report,indent=2)+"\n")
assert report["source_intacte"] and report["initial"]==report["apres_reprise"]==0 and report["apres_coupure"]>0,report
