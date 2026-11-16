"""Vérifie la présence du grand fichier dans P9 puis appelle sort depuis nsh."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from contextlib import ExitStack
import socket
from types import SimpleNamespace

ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from cli import Terminal
from journalisation import sortie
from bloc import qmp

p=argparse.ArgumentParser()
p.add_argument("image",type=Path)
p.add_argument("out",type=Path)
p.add_argument("arch",choices=["aarch64","x86_64"])
p.add_argument("sector",type=int,choices=[512,4096])
args=p.parse_args()
args.out.mkdir(parents=True,exist_ok=False)
disk=args.out/"volume.raw"
# Chaque passage reçoit une copie APFS distincte en écriture.
subprocess.run(["cp","-c",args.image,disk],check=True)
os.environ.update(NOVA_ARCH=args.arch,NOVA_RAM="256",NOVA_VCPUS="2")
start=time.monotonic()
with ExitStack() as stack:
    a,b=socket.socketpair()
    stack.enter_context(a);stack.enter_context(b)
    chan=stack.enter_context(a.makefile("rwb",buffering=0))
    monitor=SimpleNamespace(stdin=chan,stdout=chan)
    guest=stack.enter_context(Terminal(args.arch,args.sector,disk,"sort-grand",journal=args.out/"uart.serial",qmp_bloc=b.fileno()))
    b.close()
    qmp(monitor,"qmp_capabilities")
    report={"ram_qemu":qmp(monitor,"query-memory-size-summary")["base-memory"],
            "sector":args.sector,"arch":args.arch,"file":"/corpus-sort","input_bytes":320<<20}
    try:
        guest.until(b"nsh> ",timeout=300,refus=b"BLOC session terminee ok=false")
        # ls may wait for a full P9 mount on this volume.
        guest.vm.stdin.write(b"ls -l /; echo LISTE_RC=$?\n")
        text=guest.until(b"LISTE_RC=0\r\n",timeout=900,refus=b"BLOC session terminee ok=false")
        guest.until(b"nsh> ")
        report["visible"]=b"corpus-sort" in text and b"335544320" in text
        report["sort"]=sortie(guest,"sort /corpus-sort",(0,2,3,4,127))
    except (RuntimeError,AssertionError) as error:
        report["erreur"]=str(error)
    report["duree_banc_s"]=time.monotonic()-start
    (args.out/"results.json").write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps(report,ensure_ascii=False))
