"""Pénurie réelle de blocs P9 sur une copie de 2 Mio, sortie et retrait des deux enfants."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
p=argparse.ArgumentParser()
p.add_argument("recepteur",type=Path)
p.add_argument("arch",choices=["aarch64","x86_64"])
p.add_argument("sector",type=int,choices=[512,4096])
p.add_argument("out",type=Path)
p.add_argument("outil",type=Path)
p.add_argument("--blocs",type=int,default=40)
a=p.parse_args()
recv=a.recepteur.resolve(); out=a.out.resolve(); out.mkdir(parents=True,exist_ok=False)
sys.path.insert(0,str(recv/"novaOS/tests"))
from cli import Terminal
from pipe_natif import fixture
from journalisation import arret
os.environ.update(NOVA_ARCH=a.arch,NOVA_RAM="256",NOVA_VCPUS="2")
data=b"".join(f"{i%64:02x}".encode().ljust(31,b"q")+b"\n" for i in range(8192))
disk,_,_=fixture(out,a.sector,{"donnees":data})
tool=[a.outil.resolve(),disk,str(a.sector)]
fill=subprocess.run([*tool,str(a.blocs)],capture_output=True,check=True)
report=dict(arch=a.arch,secteur=a.sector,octets=len(data),volume=disk.stat().st_size,
            avant=fill.stderr.decode(),sha256=hashlib.sha256(data).hexdigest())
try:
    with Terminal(a.arch,a.sector,disk,"espace",test=True,journal=out/"espace.serial") as vm:
        vm.until(b"nsh> ",timeout=100,refus=b"BLOC session terminee ok=false")
        vm.vm.stdin.write(b"echo SDEBUT; sort donnees | uniq -c; echo SFIN_$?__fin\n")
        vm.until(b"\r\nSDEBUT\r\n")
        raw=vm.until(b"__fin\r\n",timeout=120)
        vm.until(b"nsh> ")
        report["statuts"]={k.decode():int(v) for k,v in re.findall(rb"\[pipe (sort|uniq)=(\d+)\]",raw)}
        report["diagnostics"]=[line.decode(errors="replace") for line in raw.splitlines()
                               if b"sort :" in line or b"PIPE retrait" in line or b"TMP retrait" in line]
        arret(vm,2)
except (RuntimeError,AssertionError) as e: report["erreur"]=str(e)
control=subprocess.run([*tool,"lire"],capture_output=True,check=True)
report["apres"]=control.stderr.decode()
report["source_intacte"]=control.stdout==data
(out/"resultats.json").write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n")
assert report["source_intacte"] and report.get("statuts",{}).get("sort")==2,report
