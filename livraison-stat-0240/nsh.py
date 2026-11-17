"""Vérifier la documentation et le point d’entrée sur un noyau ordinaire."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
from banc import BASE,TMP
ROOT=BASE.parent
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from cli import Terminal
from bloc import qmp
from pipe_natif import fixture
from journalisation import arret

def check(arch,sector,kernel,out):
    out.mkdir(parents=True)
    os.environ.update(NOVA_KERNEL_ELF=str(kernel),NOVA_ARCH=arch,NOVA_RAM="256",NOVA_VCPUS="1")
    with tempfile.TemporaryDirectory(dir=TMP) as temp:
        work=Path(temp)
        image,_,_=fixture(work,sector,{"temoin":b"abc"})
        log=out/"ordinaire.serial"
        owner,peer=socket.socketpair()
        with owner,peer,owner.makefile("rwb",buffering=0) as stream,Terminal(arch,sector,image,"stat-ordinaire",journal=log,qmp_bloc=peer.fileno()) as vm:
            peer.close()
            monitor=SimpleNamespace(stdin=stream,stdout=stream)
            qmp(monitor,"qmp_capabilities")
            vm.until(b"nsh> ")
            rows=[]
            def snapshot():
                by_id={d["device"]:d["stats"] for d in qmp(monitor,"query-blockstats") if "device" in d}
                d=by_id["d"]
                return [hashlib.sha256(image.read_bytes()).hexdigest(),d["wr_operations"],d["wr_bytes"],d["flush_operations"]]
            def cmd(line,rc=0,marker=None):
                vm.vm.stdin.write(line.encode()+b"\n")
                data=b""
                if line=="man stat":
                    data=vm.until(b"man stat 1/")
                    vm.vm.stdin.write(b"q")
                    data+=vm.until(b"nsh> ")
                else:
                    data=vm.until(b"\r\nnsh> ")
                if marker:assert marker in data,(line,data[-500:])
                vm.vm.stdin.write(b"echo NSH_STAT_$?__\n")
                raw=vm.until(b"\r\nnsh> ")
                assert f"NSH_STAT_{rc}__\r\n".encode() in raw,(line,raw)
                rows.append(dict(commande=line,rc=rc,uart=data.hex()))
            cmd("stat temoin",127)
            cmd("stat temoin | wc -l",3)
            for tag,marker in [("fr-CA",b"Afficher"),("fr-zzzzz",b"Display file")]:
                cmd("export LANG="+tag)
                for _ in range(12):
                    before=snapshot();time.sleep(.5)
                    if before==snapshot():break
                else:raise RuntimeError("volume actif")
                cmd("stat --help",marker=marker)
                cmd("stat --version",marker=b"0.24.0")
                cmd("man stat",marker=marker)
                after=snapshot()
                assert before==after,(before,after)
                rows.append(dict(langue=tag,avant=before,apres=after))
            arret(vm,1)
        (out/"nsh.json").write_text(json.dumps(rows,indent=1))

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("sortie",type=Path)
    a=p.parse_args()
    subprocess.run(["make","-C",ROOT/"novaOS","ARCH="+a.arch,"kernel"],check=True)
    target="aarch64-unknown-none-softfloat" if a.arch=="aarch64" else "x86_64-unknown-none"
    kernel=ROOT/"novaOS/target"/target/"release/novaos"
    assert (BASE/f"bin/stat-{a.arch}.elf").read_bytes() not in kernel.read_bytes()
    for sector in [512,4096]:check(a.arch,sector,kernel,a.sortie/str(sector))
