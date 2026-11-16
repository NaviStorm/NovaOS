"""Suspend une lecture VirtIO après les premiers octets de la fusion, puis reprend le disque."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import sys
from types import SimpleNamespace
p=argparse.ArgumentParser()
p.add_argument("recepteur",type=Path);p.add_argument("arch",choices=["aarch64","x86_64"])
p.add_argument("sector",type=int,choices=[512,4096]);p.add_argument("out",type=Path)
a=p.parse_args()
out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
sys.path.insert(0,str(a.recepteur.resolve()/"novaOS/tests"))
from cli import Terminal
from pipe_natif import fixture
from journalisation import arret
from bloc import qmp
os.environ.update(NOVA_ARCH=a.arch,NOVA_RAM="256",NOVA_VCPUS="2")
data=b"".join(f"{n%64:02x}".encode().ljust(31,b"x")+b"\n" for n in range(8192))
disk,_,_=fixture(out,a.sector,{"donnees":data})
r={"arch":a.arch,"secteur":a.sector,"octets":len(data),"source_sha256":hashlib.sha256(data).hexdigest()}
try:
    with ExitStack() as stack:
        client,server=socket.socketpair()
        stack.enter_context(client);stack.enter_context(server)
        client.settimeout(45)
        ch=stack.enter_context(client.makefile("rwb",buffering=0))
        mon=SimpleNamespace(stdin=ch,stdout=ch)
        vm=stack.enter_context(Terminal(a.arch,a.sector,disk,"fusion",test=True,
            journal=out/"fusion.serial",qmp_bloc=server.fileno()))
        server.close()
        qmp(mon,"qmp_capabilities")
        vm.until(b"nsh> ");vm.vm.stdin.write(b"sort donnees | uniq -c; echo FUSION_$?__fin\n")
        vm.until(b"PIPE premiere reception octets=",timeout=100)
        def hmp(line):
            return qmp(mon,"human-monitor-command",**{"command-line":'qemu-io d "'+line+'"'})
        r["armer"]=hmp("break read_aio fusion")
        if r["armer"]:raise ValueError("point de suspension refusé")
        # Continuer de vider l'UART pendant que la requête est suspendue.
        prior=b""
        try:
            prior=vm.until(b"PIPE fermeture lecteur",timeout=45)
            r["fermeture_lecteur"]=True
        except RuntimeError as error:
            r["attente"]=str(error)
        r["reprendre"]=hmp("resume fusion")
        if r["reprendre"]: raise ValueError("aucune requête suspendue")
        rest=vm.until(b"__fin\r\n",timeout=90)
        vm.until(b"nsh> ")
        r["statuts"]={k.decode():int(v) for k,v in re.findall(rb"\[pipe (sort|uniq)=(\d+)\]",prior+rest)}
        r["diagnostics"]=[v.decode(errors="replace") for v in (prior+rest).splitlines()
                          if b"sort :" in v or b"PIPE retrait" in v]
        # La session de stockage peut elle-même terminer en erreur après la faute.
        vm.vm.stdin.write(b"exit\n")
        vm.until(b"BLOC session terminee ok=",timeout=60)
        r["arret_invite"]=vm.until(b"\r\n").decode(errors="replace").strip()
except (RuntimeError,AssertionError,OSError,ValueError) as e: r["erreur"]=str(e)
(out/"resultats.json").write_text(json.dumps(r,ensure_ascii=False,indent=2)+"\n")
