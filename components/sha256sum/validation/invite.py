#!/usr/bin/env python3
"""Exerce le véritable lanceur et la lecture des volumes P9 jetables."""
from pathlib import Path
import argparse, hashlib, json, os, re, socket, subprocess, sys, time
from contextlib import ExitStack
from types import SimpleNamespace

root = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(root/"novaOS/tests"))
from cli import Terminal
from bloc import qmp
from pipe_natif import fixture, host
from journalisation import arret
from preuves_stockage import figer_initiale

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("arch", choices=["aarch64","x86_64"])
p.add_argument("sector", type=int, choices=[512,4096])
p.add_argument("out",type=Path)
a=p.parse_args()
out=a.out.resolve()
out.mkdir(parents=True,exist_ok=False)
os.environ.update(NOVA_VCPUS="2",NOVA_RAM="256")
payload=b"premiere\n"+bytes(range(256))*16+b"\xff\x01"
files={"fichier.bin":payload,"notice":b"livraison\n","voisin":b"prive\n"}
for name,data in files.items():(out/name).write_bytes(data)
def manifest(*names):
    return subprocess.check_output(["/opt/homebrew/bin/gsha256sum","--",*names],cwd=out,env={**os.environ,"LC_ALL":"C"})
files["SHA256SUMS"]=manifest("fichier.bin","notice")
files["DROITS"]=manifest("fichier.bin","voisin")
files["CHEMINS"]=manifest("fichier.bin").replace(b"  fichier.bin",b"  /voisin")+manifest("notice").replace(b"  notice",b"  ../voisin")
files["ABSENTS"]=manifest("notice").replace(b"  notice",b"  absent")+manifest("notice")
observations=[]
expected=hashlib.sha256(payload).hexdigest()
def run(vm,cmd,status=0,has=(),absent=(),gate=None):
    start=time.monotonic()
    vm.vm.stdin.write((cmd+"; echo SHA_RC_$?__fin\n").encode())
    prefix=b""
    if gate:
        prefix=vm.until(gate);vm.vm.stdin.write(b"+")
    body=vm.until(f"SHA_RC_{status}__fin".encode(),timeout=90)
    data=(prefix+body+vm.until(b"nsh> ")).decode(errors="replace").replace("\r\n","\n")
    matches=re.findall(r"SHA_RC_(\d+)__fin",data)
    assert matches and int(matches[-1])==status,(cmd,status,data[-3200:])
    for s in has:assert s in data,(cmd,s,data[-3200:])
    for s in absent:assert s not in data,(cmd,s,data[-3200:])
    observations.append(dict(commande=cmd,statut=status,secondes=time.monotonic()-start,sortie=data))
    (out/"observations.json").write_text(json.dumps(observations,ensure_ascii=False,indent=2)+"\n")
    return data

for altered in (False,True):
    folder=out/("modifie" if altered else "intact")
    folder.mkdir()
    source=dict(files)
    if altered:source["fichier.bin"]=payload[:-1]+bytes([payload[-1]^1])
    image,fs,lv=fixture(folder,a.sector,source)
    figer_initiale(image)
    with ExitStack() as stack:
        client,peer=socket.socketpair()
        stack.enter_context(client);stack.enter_context(peer)
        channel=stack.enter_context(client.makefile("rwb",buffering=0))
        monitor=SimpleNamespace(stdin=channel,stdout=channel)
        vm=stack.enter_context(Terminal(a.arch,a.sector,image,"sha256sum",test=True,journal=folder/"session.serial",qmp_bloc=peer.fileno()))
        peer.close()
        qmp(monitor,"qmp_capabilities")
        def snapshot():
            stats,=[d["stats"] for d in qmp(monitor,"query-blockstats") if d["device"]=="d"]
            return dict(sha256=hashlib.sha256(image.read_bytes()).hexdigest(),ecritures=stats["wr_operations"],flush=stats["flush_operations"])

        vm.until(b"nsh> ")
        run(vm,"SHA256SUM_READ_1=fichier.bin")
        run(vm,"SHA256SUM_READ_2=notice")
        if altered:
            run(vm,"sha256sum -c SHA256SUMS",1,["fichier.bin: FAILED","notice: OK"])
        else:
            run(vm,"sha256sum fichier.bin",has=[expected+"  fichier.bin"])
            run(vm,"sha256sum -c SHA256SUMS",has=["fichier.bin: OK","notice: OK"])
            run(vm,"sha256sum --quiet -c SHA256SUMS",absent=[": OK",": FAILED"])
            run(vm,"sha256sum -c DROITS",1,["fichier.bin: OK","voisin: FAILED open or read","(5)"])
            run(vm,"sha256sum --ignore-missing -c DROITS",1,["voisin: FAILED open or read","(5)"])
            run(vm,"sha256sum -c CHEMINS",1,["/voisin: FAILED","../voisin: FAILED"])
            run(vm,"SHA256SUM_READ_3=absent")
            run(vm,"sha256sum --ignore-missing -c ABSENTS",has=["notice: OK"],absent=["absent: FAILED"])
            pipe=run(vm,"cat fichier.bin | sha256sum",has=[expected+"  -","[pipe cat=0]","[pipe sha256sum=0]","admission image=19 fichiers=0","retrait noeuds=0 attentes=0 file=0"])
            roles=re.findall(r"PIPE role=(?:cat|sha256sum) pid=(\d+) isole=true",pipe)
            assert len(set(roles))==2 and sorted(re.findall(r"CLI collecte pid=(\d+)",pipe))==sorted(roles)
            run(vm,"cat notice | head -n 1",has=["livraison\n","[pipe head=0]"])
            run(vm,"cat notice | wc -l",has=["\n1\n","[pipe wc=0]"])
            run(vm,"@probe sha256-retrait fichier.bin",1,has=["(5)"],absent=[expected,hashlib.sha256(payload[:4096]).hexdigest()],gate=b"[sha256sum lecture=4096]")
            previous=snapshot()
            for _ in range(15):
                time.sleep(2)
                disk_before=snapshot()
                if disk_before==previous:break
                previous=disk_before
            else:raise AssertionError("P9 encore actif avant la fenêtre documentaire")
            run(vm,"sha256sum --help",has=["[sha256sum fichiers=0 pipe=0]","--ignore-missing"])
            run(vm,"export LANG=fr-CA")
            run(vm,"sha256sum --help",has=["[sha256sum fichiers=0 pipe=0]","Calcule ou vérifie"])
            run(vm,"sha256sum --version",has=["[sha256sum fichiers=0 pipe=0]","sha256sum (NovaOS) 0.24.0"])
            vm.vm.stdin.write(b"man sha256sum; echo MAN_RC_$?__fin\n")
            frame=vm.until(b"q quitter")
            assert "Calculer ou vérifier".encode() in frame
            vm.vm.stdin.write(b"q");vm.until(b"MAN_RC_0__fin");vm.until(b"nsh> ")
            disk_after=snapshot()
            assert disk_before==disk_after,(disk_before,disk_after)
            (folder/"documentation-disque.json").write_text(json.dumps([disk_before,disk_after]))
        arret(vm,2)
    records=host(image,"inspect","512",str(a.sector))
    slot=max(re.findall(r"slot=(\d+) génération=(\d+)".encode(),records),key=lambda x:int(x[1]))[0]
    host(image,"verify","512",str(a.sector),fs,lv)
    for name,data in source.items():
        assert host(image,"salvage","512",str(a.sector),fs,lv,slot,"/"+name)==data
print(a.arch,a.sector,len(observations),"observations vérifiées")
