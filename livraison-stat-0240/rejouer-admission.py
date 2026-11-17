"""Contrôler les cas d’admission sur une copie des médias de recette archivés."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch

from banc import BASE, TMP
sys.path.insert(0,str(BASE.parent/"novaOS/tests"))
from cli import Terminal
from bloc_qemu import command
from journalisation import arret

def check(cell,out):
    info=json.loads((cell/"mesures.json").read_text())
    arch,sector=info["arch"],info["secteur"]
    rows=[]
    with tempfile.TemporaryDirectory(dir=TMP,prefix="stat-admission-") as wd:
        wd=Path(wd)
        disk=wd/"copie.raw"
        disk.write_bytes(gzip.decompress((cell/"p9-initial.raw.gz").read_bytes()))
        iso=wd/"recette.iso"
        iso.write_bytes(gzip.decompress((cell/"terminal.iso.gz").read_bytes()))
        assert hashlib.sha256(iso.read_bytes()).hexdigest()==info["iso"]
        def machine(*args,**kwargs):
            env=dict(os.environ,NOVA_ARCH=arch,NOVA_VCPUS="1",NOVA_RAM="256",NOVA_ISO=str(iso))
            return command(arch,sector,disk,controls=["-display","none","-monitor","none","-serial","stdio"]),env
        # Terminal pilote le média déjà vérifié. Chaque écriture porte sur la copie privée.
        with patch("cli.machine",machine),Terminal(arch,sector,disk,"stat-admission") as vm:
            vm.until(b"nsh> ")
            tests=[("touch 'nom espace'",0,None),("@stat --pr='%Qn=%s' 'nom espace'",0,b"'nom espace'=0"),
                   ("@stat -c%n f0 -",5,b"stat:"),("@stat -c%n f0 absent f1",20,b"stat:"),
                   ("@stat --format=%n --printf='%s' -- f0",0,b"3"),
                   ("@stat --printf=x -c%n f1",0,b"f1\r\n"),
                   ("@stat --cached=",3,b"ambiguous"),("@stat",3,b"missing operand"),
                   ("@stat --f",3,b"ambiguous"),("@stat --dere f0",0,b"regular file"),
                   ("@stat --printf='a%9%' f0",3,b"invalid directive")]
            for line,rc,marker in tests:
                vm.vm.stdin.write(line.encode()+b"\n")
                raw=vm.until(b"\r\nnsh> ")
                vm.vm.stdin.write(b"echo ADMISSION_$?__\n")
                response=vm.until(b"\r\nnsh> ")
                assert f"ADMISSION_{rc}__\r\n".encode() in response,(line,raw,response)
                assert not marker or marker in raw,(line,raw,marker)
                rows.append(dict(cmd=line,rc=rc,uart=raw.hex()))
            arret(vm,1)
        out.write_text(json.dumps(dict(iso=info["iso"],essais=rows),indent=1))
if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("cellule",type=Path)
    p.add_argument("sortie",type=Path)
    a=p.parse_args()
    check(a.cellule,a.sortie)
