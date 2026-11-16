#!/usr/bin/env python3
"""Rejoue le témoin nsh et conserve le média effectivement démarré."""
import argparse
import os
import re
import socket
import time
from contextlib import ExitStack
from types import SimpleNamespace
import sys
import json
from pathlib import Path
root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root/"novaOS/tests"))
from bloc import qmp
from cli import Terminal
from pipe_natif import fixture
from journalisation import arret
from preuves_stockage import figer_initiale, empreinte
from subprocess import run, PIPE

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("architecture", choices=["aarch64", "x86_64"])
p.add_argument("secteur", type=int, choices=[512, 4096])
p.add_argument("sortie", type=Path, help="répertoire neuf")
a = p.parse_args()
out = a.sortie.resolve()
out.mkdir(parents=True, exist_ok=False)
os.environ.update(NOVA_RAM="256", NOVA_VCPUS="2")
image, _, _ = fixture(out, a.secteur, {"a.bin":b"\0\n\xff", "b.bin":b"\0\n\xfe"})
figer_initiale(image)
serial = out/"session.serial"
rows = []
with ExitStack() as stack:
    client,peer=socket.socketpair()
    stack.enter_context(client)
    stack.enter_context(peer)
    channel=stack.enter_context(client.makefile("rwb",buffering=0))
    monitor=SimpleNamespace(stdin=channel,stdout=channel)
    vm=stack.enter_context(Terminal(a.architecture,a.secteur,image,"cmp",journal=serial,qmp_bloc=peer.fileno()))
    peer.close()
    qmp(monitor,"qmp_capabilities")
    ram=qmp(monitor,"query-memory-size-summary")
    assert ram["base-memory"]==256<<20
    def snapshot():
        stats,=[d["stats"] for d in qmp(monitor,"query-blockstats") if d["device"]=="d"]
        return dict(sha256=empreinte(image),writes=stats["wr_operations"],flush=stats["flush_operations"])
    def command(cmd,status,marker):
        vm.vm.stdin.write((cmd+"; echo CMP_RC_$?__fin\n").encode())
        trace=vm.until(b"\r\nnsh> ")
        m=re.search(rb"CMP_RC_(\d+)__fin",trace)
        actual=int(m[1]) if m else None
        assert actual==status and marker.encode() in trace,(cmd,trace)
        rows.append(dict(commande=cmd,statut_nsh=actual,trace=trace.hex()))
    vm.until(b"nsh> ")
    command("cmp a.bin b.bin",127,"commande inconnue")
    command("cat a.bin | cmp -s b.bin -",None,"saisie refusée")
    before=snapshot()
    for _ in range(15):
        time.sleep(1)
        latest=snapshot()
        if before==latest:break
        before=latest
    else:raise RuntimeError("P9 toujours actif")
    command("cmp --bad absent",2,"unrecognized option '--bad'")
    for lang,marker,prompt in [("en","Compare two inputs",b"q quit"),("fr-CA","Comparer deux entr",b"q quitter"),("fr--CA","Compare two inputs",b"q quit")]:
        vm.line("export LANG="+lang)
        command("cmp --help",0,marker)
        command("cmp --version",0,"cmp (NovaOS) 0.24.0")
        vm.vm.stdin.write(b"man cmp; echo MAN_RC_$?__fin\n")
        screen=vm.until(prompt)
        assert marker.encode() in screen,screen
        vm.vm.stdin.write(b"q")
        vm.until(b"MAN_RC_0__fin")
        vm.until(b"nsh> ")
        rows.append(dict(commande="man cmp",langue=lang,statut_nsh=0))
    after=snapshot()
    (out/"disque-doc.json").write_text(json.dumps([before,after]))
    assert before==after,(before,after)
    arret(vm,2)
iso = serial.with_suffix(".iso")
kernel = out/"noyau-extrait.elf"
run(["xorriso","-osirrox","on","-indev",iso,"-extract","/boot/novaos.elf",kernel],check=True,stderr=PIPE)
target = "aarch64-unknown-none-softfloat" if a.architecture=="aarch64" else "x86_64-unknown-none"
build = root/"novaOS/target"/target/"release"
data = kernel.read_bytes()
assert empreinte(kernel) == empreinte(build/"novaos")
embedded = {}
for name in ["nsh","cat"]:
    candidates=[e for e in build.glob("build/novaos-*/out/"+name+".elf") if e.read_bytes() in data]
    assert candidates, name
    embedded[name]=empreinte(candidates[0])
cmp = Path(__file__).resolve().parent/"bin"/("cmp-"+a.architecture+".elf")
embedded["cmp_present"] = cmp.read_bytes() in data
assert not embedded["cmp_present"]
(out/"temoin.json").write_text(json.dumps(dict(iso=empreinte(iso),kernel=empreinte(kernel),
    elf=embedded,ram_qmp=ram,observations=rows),indent=2)+"\n")
print(a.architecture, a.secteur, [(r["commande"],r["statut_nsh"]) for r in rows])
