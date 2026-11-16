"""Deux démarrages TCG sur P9 jetable. Le second inclut un témoin de délégation fichier."""
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time
from types import SimpleNamespace

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from cli import Terminal
from bloc import qmp
from journalisation import attend,sortie
from pipe_natif import fixture, host

def sha(path):
    with path.open("rb") as f: return hashlib.file_digest(f,"sha256").hexdigest()

def main():
    arch,geometry,folder=sys.argv[1:]
    sector=int(geometry)
    assert sector in (512,4096)
    target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[arch]
    out=Path(folder).resolve()
    out.mkdir(parents=True,exist_ok=False)
    tmp=Path(os.environ["TMPDIR"])
    os.environ.update(NOVA_ARCH=arch,NOVA_RAM="256",NOVA_VCPUS="2")
    production=ROOT/"novaOS/target"/target/"release/novaos"
    lab=tmp/("labo-"+arch)/"target"/target/"release/novaos"
    assert production.is_file() and lab.is_file(), "reconstruire les noyaux ordinaire et de recette"
    source=bytes(range(256))*4+b"\xfe\x00abc\n"
    original=b"destination protegee\n"
    image,fs,lv=fixture(out,sector,{"entree":source,"occupe":original,"vide":b""})
    rows=[]
    timings=[]
    os.environ["NOVA_KERNEL_ELF"]=str(production)
    with ExitStack() as stack:
        a,b=socket.socketpair()
        stack.enter_context(a);stack.enter_context(b)
        channel=stack.enter_context(a.makefile("rwb",buffering=0))
        monitor=SimpleNamespace(stdin=channel,stdout=channel)
        vm=stack.enter_context(Terminal(arch,sector,image,"tee",journal=out/"nsh.serial",qmp_bloc=b.fileno()))
        b.close()
        qmp(monitor,"qmp_capabilities")
        vm.until(b"nsh> ")
        attend(lambda:sortie(vm,"service status 16"),lambda s:"Current state: RUNNING" in s)
        def state():
            stats,=[r["stats"] for r in qmp(monitor,"query-blockstats") if r["device"]=="d"]
            return [sha(image),stats["wr_bytes"],stats["wr_operations"],stats["flush_operations"]]
        for _ in range(25):
            before=state();time.sleep(.5)
            if state()==before:break
        else:raise RuntimeError("volume actif")
        for lang in ("fr-CA","en-US","fr--CA"):
            vm.line("export LANG="+lang)
            fr=lang=="fr-CA"
            wording="Copier" if fr else "Copy"
            for arg in ("--help","--he","-aip --help","--version","--ver"):
                text=sortie(vm,"tee "+arg)
                assert ("nova-cli 0.20.0" if arg.startswith("--ver") else wording) in text,text
                rows.append([lang,arg,0])
            vm.vm.stdin.write(b"man tee; echo manual=$?\n")
            frame=vm.until(b"q quitter" if fr else b"q quit  / search")
            assert wording.encode() in frame
            vm.vm.stdin.write(b"q")
            vm.until(b"manual=0");vm.until(b"nsh> ")
            rows.append([lang,"man tee",0])
        assert "invalid option" in sortie(vm,"tee -h",1)
        after=state()
        assert before==after,(before,after)
        vm.vm.stdin.write(b"nsh -c 'cat entree | tee sortie'; echo admission=$?\n")
        vm.until(b"admission=3");vm.until(b"nsh> ")
        sortie(vm,"test -e sortie",1)
        assert "5" in sortie(vm,"cat entree | wc -l")
    os.environ["NOVA_KERNEL_ELF"]=str(lab)
    with Terminal(arch,sector,image,"tee-lab",journal=out/"labo.serial") as vm:
        vm.until(b"nsh> ")
        for src,dest,rc in [("entree","copie",0),("vide","zero",0),("entree","occupe",1)]:
            size=len(source) if src=="entree" else 0
            marker=f"TEE-LAB matrices=33 p9={rc} stdout={size} miroir={size}"
            assert marker in sortie(vm,"cp "+src+" "+dest)
            rows.append(["temoin",src+" "+dest,marker])
        for n in range(4):
            for outputs in ((1,2) if n%2==0 else (2,1)):
                dest="mesure"+str(outputs)
                vm.vm.stdin.write(f"cp entree {dest}; echo teeMesure=$?\n".encode())
                vm.until(b"TEE-DEBUT\r\n")
                begin=time.monotonic_ns()
                vm.until(b"TEE-FIN\r\n")
                elapsed=time.monotonic_ns()-begin
                vm.until(b"teeMesure=0");vm.until(b"nsh> ")
                timings.append(dict(passage=n,sorties=outputs,ns=elapsed,octets_lus=len(source),octets_ecrits=len(source)*outputs))
                if outputs==2 and n<3:sortie(vm,"rm mesure2")
    anchors=re.findall("slot=(\\d+) génération=(\\d+)".encode(),host(image,"inspect","512",geometry))
    slot=max(anchors,key=lambda row:int(row[1]))[0]
    readback={}
    for name,expected in {"entree":source,"occupe":original,"copie":source,"zero":b"","mesure2":source}.items():
        actual=host(image,"salvage","512",geometry,fs,lv,slot,"/"+name)
        assert actual==expected,name
        readback[name]=hashlib.sha256(actual).hexdigest()
    (out/"resultats.json").write_text(json.dumps(dict(cas=rows,mesures=timings,avant=before,apres=after,admission_nsh=3,
        relecture=readback,noyau_ordinaire=sha(production),noyau_labo=sha(lab),image=sha(image)),indent=2)+"\n")
    print(arch,sector,"OK")

if __name__=="__main__":main()
