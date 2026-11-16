#!/usr/bin/env python3
"""Mesures P9 et pipe sur un fichier creux de 128 Mio + 1 avec 128 Mio de RAM."""
import argparse, hashlib, json, os, re, subprocess, sys, time
from pathlib import Path
from statistics import median
root=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(root/"novaOS/tests"))
from cli import HOST, Terminal
from pipe_natif import host
from journalisation import arret
p=argparse.ArgumentParser(description=__doc__)
p.add_argument("arch",choices=["aarch64","x86_64"])
p.add_argument("sector",type=int,choices=[512,4096])
p.add_argument("out",type=Path)
p.add_argument("--above-ram",action="store_true")
a=p.parse_args()
out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
os.environ.update(NOVA_VCPUS="2",NOVA_RAM="128")
sizes={"court":16385,"moyen":131073}
if a.above_ram:sizes["grand"]=(128<<20)+1
cmd=[];hashes={}
for name,size in sizes.items():
    cmd += [f"create /{name}",f"write /{name} {size-1} b7"]
    h=hashlib.sha256()
    for off in range(0,size-1,1<<20):h.update(bytes(min(1<<20,size-1-off)))
    h.update(b"\xb7");hashes[name]=h.hexdigest()
batch=[]
for n in range(12):
    name=f"p{n}"
    data=bytes([n])*16
    cmd += [f"create /{name}",f"write /{name} 0 {data.hex()}"]
    batch.append(f"{hashlib.sha256(data).hexdigest()}  {name}\n")
manifest="".join(batch).encode()
cmd+=["create /LOTS",f"write /LOTS 0 {manifest.hex()}"]
source=out/"genese.raw";image=out/"session.raw"
with source.open("xb") as f:
    result=subprocess.run([HOST,"script","512",str(a.sector)],input=("\n".join(cmd)+"\n").encode(),stdout=f,stderr=subprocess.PIPE,check=True)
fs,lv=re.search(rb"FS=(\w+) LV=(\w+)",result.stderr).groups()
with image.open("xb") as f:host(source,"migrate-content-p9","512",str(a.sector),fs,lv,output=f)
report=dict(architecture=a.arch,secteur=a.sector,ram_octets=128<<20,vcpu=2,emulation="TCG",volume_octets=image.stat().st_size,
    references=hashes,mesures=[],resume=[])
def save():(out/"debits.json").write_text(json.dumps(report,indent=2,ensure_ascii=False)+"\n")
def execute(cmd,rc=0,timeout=120):
    vm.vm.stdin.write((cmd+"; echo PERF_RC_$?__fin\n").encode())
    start=time.monotonic()
    raw=vm.until(f"PERF_RC_{rc}__fin".encode(),timeout=timeout)
    seconds=time.monotonic()-start
    vm.until(b"nsh> ")
    return raw.replace(b"\r\n",b"\n"),seconds
with Terminal(a.arch,a.sector,image,"sha-debit",test=True,journal=out/"debit.serial") as vm:
    vm.until(b"nsh> ")
    for i in range(12):execute(f"SHA256SUM_READ_{i+1}=p{i}")
    tasks=[("sha256sum -c --quiet LOTS",len(manifest)+12*16,None)]
    for name,size in sizes.items():
        tasks.extend([(f"sha256sum {name}",size,hashes[name]+"  "+name),(f"cat {name} | sha256sum",size,hashes[name]+"  -")])
    for cmd,size,expected in tasks:
        speeds=[]
        for i in range(1 if size>1<<20 else 3):
            raw,seconds=execute(cmd,timeout=7200)
            if expected:assert expected.encode() in raw
            if "|" in cmd:assert b"[pipe sha256sum=0]" in raw and b"[pipe cat=0]" in raw
            pages=int(re.search(rb"CLI spawn image=19 pid=\d+ cycles=\d+ pages=(\d+)",raw)[1])
            speed=size/seconds/(1<<20)
            speeds.append(speed)
            report["mesures"].append(dict(commande=cmd,octets=size,secondes=seconds,mio_s=speed,pages_admission=pages))
            save()
        report["resume"].append(dict(commande=cmd,n=len(speeds),mediane_mio_s=median(speeds),min_mio_s=min(speeds),max_mio_s=max(speeds)))
        save()
    arret(vm,2)
print(a.arch,a.sector,len(report["mesures"]),"mesures")
