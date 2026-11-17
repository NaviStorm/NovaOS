"""QEMU TCG. Session nsh sur P9 et témoin SDK du moteur, chacun avec une ISO privée."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import socket
import subprocess
import sys
import time
from types import SimpleNamespace

ROOT=Path(__file__).resolve().parents[2]
DELIVERY=ROOT/"livraison-tr-0240"
BIN=Path(os.environ.get("NOVA_TR_BIN_DIR", DELIVERY/"bin")).resolve()
OS=ROOT/"novaOS"
sys.path.insert(0,str(OS/"tests"))
from cli import Terminal
from bloc import qmp
from bloc_qemu import command
from journalisation import attend,sortie
from pipe_natif import fixture
from vecteurs import generate

def sha(p):
    with p.open("rb") as f: return hashlib.file_digest(f,"sha256").hexdigest()

def jsonfile(path,obj): path.write_text(json.dumps(obj,indent=2,ensure_ascii=False)+"\n")

def cmd(args,cwd=ROOT,**kwargs):
    subprocess.run(args,cwd=cwd,check=True,**kwargs)

def check_iso(iso,kernel,out,apps,absent):
    extract=out/"noyau-demarre.elf"
    cmd(["xorriso","-osirrox","on","-indev",iso,"-extract","/boot/novaos.elf",extract])
    assert sha(extract)==sha(kernel)
    image=extract.read_bytes()
    manifest={}
    for app in apps:
        data=app.read_bytes()
        assert data in image, app
        manifest[app.name]=sha(app)
    assert absent.read_bytes() not in image
    return dict(iso=sha(iso),kernel=sha(extract),embarques=manifest,tr_production_absent=sha(absent))

def archive_apps(kernel,examples=()):
    build=kernel.parent/"build"
    catalogs=[p for p in build.glob("novaos-*/out") if (p/"nsh.elf").exists() and (p/"nsh.elf").read_bytes() in kernel.read_bytes()]
    assert catalogs
    folder=catalogs[0]
    names=["nsh","pwd","ls","cat","head","mkdir","rm","probe","cp","mv","touch","more","wc","tail","rmdir","vi"]
    return [folder/(n+".elf") for n in names]+list(examples)

def doc(arch,sector,out,kernel,image):
    out.mkdir()
    os.environ["NOVA_KERNEL_ELF"]=str(kernel)
    rows=[]
    with ExitStack() as stack:
        a,b=socket.socketpair()
        stack.enter_context(a);stack.enter_context(b)
        channel=stack.enter_context(a.makefile("rwb",buffering=0))
        monitor=SimpleNamespace(stdin=channel,stdout=channel)
        vm=stack.enter_context(Terminal(arch,sector,image,"tr",journal=out/"nsh.serial",qmp_bloc=b.fileno()))
        b.close()
        qmp(monitor,"qmp_capabilities")
        ram=qmp(monitor,"query-memory-size-summary")["base-memory"]
        vm.until(b"nsh> ")
        attend(lambda:sortie(vm,"service status 16"),lambda s:"Current state: RUNNING" in s)
        def state():
            stats,=[r["stats"] for r in qmp(monitor,"query-blockstats") if r["device"]=="d"]
            return [sha(image),stats["wr_bytes"],stats["wr_operations"],stats["flush_operations"]]
        for _ in range(20):
            before=state();time.sleep(.5)
            if before==state(): break
        else: raise RuntimeError("volume encore actif")
        for tag,fr in [("fr-CA",True),("en-US",False),("fr--CA",False)]:
            vm.line("export LANG="+tag)
            wording="Traduire" if fr else "Translate"
            for args in ["--help","--he","-ds --help","--version","--ver"]:
                result=sortie(vm,"tr "+args)
                assert ("nova-cli" if args.startswith("--ver") else wording) in result
                rows.append([tag,args,0])
            result=sortie(vm,"tr --inconnue",1)
            assert "unrecognized option" in result
            rows.append([tag,"--inconnue",1])
            vm.vm.stdin.write(b"man tr; echo trman=$?\n")
            frame=vm.until(b"q quitter" if fr else b"q quit  / search")
            assert wording.encode() in frame
            vm.vm.stdin.write(b"q")
            vm.until(b"trman=0"); vm.until(b"nsh> ")
            rows.append([tag,"man tr",0])
        after=state()
        assert after==before,(before,after)
        for line in [
            "cat entree | tr '[:lower:]' '[:upper:]'",
            "cat entree | tr -s ' '",
            "cat entree | tr --inconnue",
            "cat donnees.txt | tr 'a-z' 'A-Z'",
        ]:
            # La validation syntaxique du script interne rend son statut au shell extérieur.
            inner=line.replace("'",'"')
            vm.vm.stdin.write(f"nsh -c '{inner}'; echo lancement=$?\n".encode())
            vm.until(b"lancement=3"); vm.until(b"nsh> ")
            rows.append(["nsh",line,3])
        assert "259" in sortie(vm,"cat entree | wc -l")
        assert "abc-123" in sortie(vm,"cat donnees.txt")
        rows.extend([["nsh", "cat entree | wc -l", 0], ["nsh", "cat donnees.txt", 0]])
    info=check_iso(out/"nsh.iso",kernel,out,archive_apps(kernel),BIN/f"tr-{arch}.elf")
    jsonfile(out/"resultats.json",dict(cases=rows,ram=ram,avant=before,apres=after,media=info))
    return ram

def sdk(arch,sector,out,kernel,elf,image,ram,group,source,expected_cases):
    out.mkdir()
    iso=out/"sdk.iso"
    os.environ.update(NOVA_KERNEL_ELF=str(kernel),NOVA_ISO=str(iso))
    cmd(["sh",OS/"scripts/iso.sh","sdk-example",iso])
    info=check_iso(iso,kernel,out,archive_apps(kernel,[elf]),BIN/f"tr-{arch}.elf")
    shutil.copyfile(elf,out/"temoin.elf")
    opts=["-display","none","-monitor","none","-serial","stdio"]
    argv=command(arch,sector,image,readonly=True,controls=opts)
    jsonfile(out/"configuration.json",dict(argv=argv,arch=arch,secteur=sector,ram_octets=ram,vcpu=2,media=info))
    events=[]
    pending=b""
    started={}
    matrix=None
    with (out/"sdk.serial").open("xb") as log, subprocess.Popen(argv,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0) as vm:
        try:
            deadline=time.monotonic()+360
            done=False
            while time.monotonic()<deadline and not done:
                if not select.select([vm.stdout],[],[],1)[0]: continue
                data=os.read(vm.stdout.fileno(),65536)
                if not data: break
                now=time.monotonic_ns()
                log.write(data);log.flush()
                pending+=data
                while b"\n" in pending:
                    line,pending=pending.split(b"\n",1)
                    if b"Panique du noyau" in line or b"TR FAIL" in line: raise RuntimeError(line)
                    if check:=re.search(rb"TR MATRIX cases=(\d+)",line): matrix=int(check[1])
                    mark=re.search(rb"TR (BEGIN|END) (.+)",line)
                    if mark:
                        v={k.decode():int(n) for k,n in re.findall(rb"(\w+)=(\d+)",mark[2])}
                        key=v["mode"],v["case"]
                        if mark[1]==b"BEGIN": started[key]=(now,v["cases"])
                        else:
                            start,cases=started.pop(key)
                            events.append(dict(**v,seconds=(now-start)/1e9,matrix=cases))
                    if b"SDK exemple admis sortie=0 restitution=OK" in line: done=True
            assert done and not started and (matrix if group==2 else len(events)==8),(done,started,len(events))
        finally:
            vm.kill();vm.wait()
    if events: assert max(row["input"] for row in events)>ram
    observed_cases = [row["matrix"] for row in events] if events else [matrix]
    assert all(count == expected_cases for count in observed_cases), (observed_cases, expected_cases)
    jsonfile(out/"matrice.json",dict(cas=expected_cases,statut=0,source=source.name,
             source_sha256=sha(source),moteur_et_lecture_de_trame=True))
    jsonfile(out/"mesures.json",events)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("sector",type=int,choices=[512,4096])
    p.add_argument("out",type=Path)
    p.add_argument("--large",action="store_true",help="mesurer aussi le moteur SDK au-delà de 256 Mio")
    p.add_argument("--matrice", type=Path, default=DELIVERY/"preuves/differentiel.jsonl.gz",
                   help="comparaisons GNU à rejouer dans le témoin SDK")
    a=p.parse_args()
    out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    tmp=Path(os.environ.get("NOVAOS_TMPDIR", ROOT.parent/"tmp"))
    os.environ.update(NOVAOS_TMPDIR=str(tmp),TMPDIR=str(tmp)+"/",NOVA_ARCH=a.arch,NOVA_RAM="256",NOVA_VCPUS="2",
        RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[a.arch]
    cache=tmp/("tr-0240-"+a.arch);cache.mkdir(exist_ok=True)
    cmd(["make","-C","novaOS","ARCH="+a.arch,"kernel"])
    cmd(["novaFS/lab/run","build","--offline","--bin","novafs"],env=dict(os.environ,NOVAFS_TARGET="aarch64-apple-darwin"))
    image,_,_=fixture(out,a.sector,{"entree":b"ab  C"+b" "*4096+b"Z\n"*259, "donnees.txt":b"abc-123\n"})
    kernel=OS/"target"/target/"release/novaos"
    ram=doc(a.arch,a.sector,out/"nsh",kernel,image)
    vectors=cache/"vecteurs.bin"
    expected_cases = generate(a.matrice, vectors)
    assert expected_cases > 0
    report=json.loads((DELIVERY/"preuves/mesures-mac.json").read_text())
    counts=cache/"comptes.rs"
    counts.write_text(str([[r["output"] for r in report["rows"][i:i+4]] for i in range(0,16,4)]))
    for group in (range(2) if a.large else [2]):
        flags=f"-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T{OS}/user/novafs/service.ld"
        env=dict(os.environ,CARGO_TARGET_DIR=str(cache/"temoin"),RUSTFLAGS=flags,NOVA_TR_GROUP=str(group),NOVA_TR_VECTORS=str(vectors),NOVA_TR_COUNTS=str(counts))
        cmd(["rustup","run","stable","cargo","build","--offline","--locked","--release","--example","tr-temoin","--features","qualification-tr","--target",target],cwd=ROOT/"components",env=env)
        elf=cache/"temoin"/target/"release/examples/tr-temoin"
        env=dict(os.environ,CARGO_TARGET_DIR=str(cache/f"sdk-{group}"),NOVA_THREAD_PROBE_ELF=str(elf))
        cmd(["rustup","run","stable","cargo","build","--offline","--locked","--release","-p","novaos","--features","thread-probe","--target",target],cwd=OS,env=env)
        sdk(a.arch,a.sector,out/f"sdk-{group}",cache/f"sdk-{group}"/target/"release/novaos",elf,image,ram,group,a.matrice,expected_cases)

if __name__=="__main__":main()
