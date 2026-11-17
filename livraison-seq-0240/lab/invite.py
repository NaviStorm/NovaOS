"""Recette du vrai ELF (catalogue SDK) et de la documentation du vrai nsh."""
import argparse
import base64
from contextlib import ExitStack
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import struct
import subprocess as sp
import sys
import tarfile
import tempfile
import statistics
import time
from types import SimpleNamespace

from options import HERE, NOVA, invoke
ROOT=HERE.parent
OS=ROOT/"novaOS"
sys.path.insert(0,str(OS/"tests"))
from cli import Terminal
from bloc import qmp
from bloc_qemu import command
from journalisation import attend, sortie
from pipe_natif import fixture

TARGETS={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def rows(path):
    with gzip.open(path,"rt") as f:
        for line in f:
            row=json.loads(line)
            if row["cas"]!="différentiel" or row["locale"]!="C": continue
            args=list(map(base64.b64decode,row["argv"]))
            if sum(2+len(a) for a in args)+1>2560: continue
            status,out,err=row["gnu"]
            out=base64.b64decode(out)
            err=base64.b64decode(err).replace(b"gseq:",b"seq:",1)
            # À l'entrée, le runtime v1 valide UTF-8 avant d'appeler seq.
            try:
                for arg in args: arg.decode()
            except UnicodeDecodeError:
                yield row["id"],args,6,(3,b"",b""),"argv UTF-8"
                continue
            if any(b<32 and b!=10 for b in out):
                # -s mono-octet : le premier bloc est refusé en entier.
                yield row["id"],args,6,(1,b"", "seq: argument invalide\n".encode()),"contrôle terminal"
                continue
            yield row["id"],args,6,(status,out,err),"GNU C"
    for flag in ("--help","--version"):
        for tag in ("en","fr"):
            args=[flag] if tag=="en" else [flag,"--","nsh-doc=fr"]
            expected=invoke(NOVA,[flag],"fr_FR.UTF-8" if tag=="fr" else "C")
            yield f"{flag}-{tag}",list(map(str.encode,args)),6,expected,"ARC-026"
    yield "sortie-refusée",[b"2"],4,(1,b"","seq: accès refusé\n".encode()),"E/S"


def prepare(work, arch, vectors):
    # Seuls le contrôleur d'essai et son observateur sont copiés/modifiés.
    # Les sources de production et les points d'entrée de nsh restent ceux du worktree.
    tree=work/"src"
    tree.mkdir()
    for name in ["components","novaFS"]:
        (tree/name).symlink_to(ROOT/name,target_is_directory=True)
    kernel=tree/"novaOS"
    kernel.mkdir()
    for p in OS.iterdir():
        if p.name=="kernel":
            shutil.copytree(p,kernel/p.name,ignore=shutil.ignore_patterns("target"))
        elif p.name not in ("target","build"):
            (kernel/p.name).symlink_to(p,target_is_directory=p.is_dir())
    source=kernel/"kernel/src/sched/process/ipc/thread_probe.rs"
    text=source.read_text()
    begin=text.index("pub(crate) fn run_example()")
    end=text.index("pub(crate) fn run()",begin)
    source.write_text(text[:begin]+(HERE/"lab/controleur.rs").read_text()+"\n"+text[end:])
    buf=bytearray()
    for _,args,ports,_,_ in vectors:
        argv=bytes([len(args)])+b"".join(struct.pack("<H",len(a))+a for a in args)
        data=struct.pack("<5Q",0,0,0,0,len(argv))+argv
        buf+=struct.pack("<HB",len(data),ports)+data
    source.with_name("seq-argv.bin").write_bytes(buf)
    p=kernel/"kernel/src/sched/process/syscall.rs"
    text=p.read_text()
    anchor="    LINE_OPEN.store(!text.ends_with('\\n'), Ordering::Relaxed);"
    assert text.count(anchor)==1
    p.write_text(text.replace(anchor,'''    if ipc::thread_probe::TRACER.load(Ordering::Relaxed) { for octets in bytes.chunks(16) {
        hal::log(format_args!("SEQHEX {} {right} {:02x?}",pid.number,octets));
    } }
'''+anchor))
    elf=HERE/f"bin/seq-{arch}.elf"
    cache=work/"cible"
    env=dict(os.environ,CARGO_TARGET_DIR=str(cache),NOVA_THREAD_PROBE_ELF=str(elf))
    sp.run(["rustup","run","stable","cargo","build","--offline","--release","--locked","--target",TARGETS[arch],"--features","thread-probe"],
           cwd=kernel,env=env,check=True)
    image=cache/TARGETS[arch]/"release/novaos"
    assert elf.read_bytes() in image.read_bytes()
    return image


def iso_check(iso,kernel,work):
    extracted=work/(iso.stem+"-extrait.elf")
    sp.run(["xorriso","-osirrox","on","-indev",iso,"-extract","/boot/novaos.elf",extracted],check=True,stdout=sp.DEVNULL,stderr=sp.DEVNULL)
    assert sha(extracted)==sha(kernel)
    return {"iso":sha(iso),"noyau":sha(extracted)}


def sdk(arch,sector,work,image,kernel,vectors):
    iso=work/"sdk.iso"
    env=dict(os.environ,NOVA_ISO=str(iso),NOVA_KERNEL_ELF=str(kernel))
    sp.run(["sh",OS/"scripts/iso.sh","sdk-example",iso],env=env,check=True,stdout=sp.DEVNULL)
    media=iso_check(iso,kernel,work)
    argv=command(arch,sector,image,readonly=True,controls=["-display","none","-monitor","none","-serial","file:"+str(work/"sdk.serial")])
    before=sha(image)
    with (work/"qemu.log").open("wb") as err,sp.Popen(argv,env=env,stdin=sp.DEVNULL,stdout=err,stderr=err) as vm:
        try:
            log=work/"sdk.serial"
            end=time.monotonic()+300
            while time.monotonic()<end and vm.poll() is None:
                time.sleep(.25)
                if not log.exists(): continue
                text=log.read_bytes()
                if b"Panique du noyau" in text: raise RuntimeError(text[-1000:])
                if f"SEQTERM {len(vectors)}".encode() in text: break
            else: raise TimeoutError("recette SDK incomplète")
        finally: vm.kill();vm.wait()
    log=log.read_bytes()
    results=[]
    for i,(key,args,ports,expected,mode) in enumerate(vectors):
        start=re.search(rb"SEQBEGIN "+str(i).encode()+rb" pid=(\d+) ports=(\d+)",log)
        finish=re.search(rb"SEQEND "+str(i).encode()+rb" Exit\((\d+)\)",log)
        assert start and finish and int(start[2])==ports,(i,key,log[-400:])
        frames=re.findall(rb"SEQHEX (\d+) ([24]) \[([0-9a-f, ]+)\]",log[start.end():finish.start()])
        out,err=bytearray(),bytearray()
        for pid,port,data in frames:
            assert pid==start[1]
            (out if port==b"2" else err).extend(bytes.fromhex(data.decode().replace(",","")))
        got=int(finish[1]),out,err
        result=dict(id=key,pid=int(start[1]),oracle=mode,ok=got==expected,
                    statut=got[0],stdout=base64.b64encode(out).decode(),stderr=base64.b64encode(err).decode())
        results.append(result)
    perf=[]
    for n in [1000,10000]:
        expected=invoke("/opt/homebrew/bin/gseq",[str(n)],"C")[1]
        measures=[]
        for turn in range(4):
            trial=re.search(fr"SEQBENCH {n} {turn}\r\n(.*?)SEQTIME (\d+) (\d+)\r\n".encode(),log,re.S)
            assert trial and trial[1].replace(b"\r\n",b"\n")==expected
            measures.append(int(trial[2])/int(trial[3]))
        perf.append(dict(valeurs=n,secondes=measures[1:],echauffement=measures[0],
                         mediane=statistics.median(measures[1:]),sha256=hashlib.sha256(expected).hexdigest(),octets=len(expected)))
    assert before==sha(image)
    return dict(media=media,elf=sha(HERE/f"bin/seq-{arch}.elf"),p9=before,commande=argv,cas=results,performance=perf)


def shell(arch,sector,work,image):
    kernel=OS/"target"/TARGETS[arch]/"release/novaos"
    os.environ["NOVA_KERNEL_ELF"]=str(kernel)
    results=[]
    with ExitStack() as stack:
        a,b=socket.socketpair()
        stack.enter_context(a);stack.enter_context(b)
        channel=stack.enter_context(a.makefile("rwb",buffering=0))
        monitor=SimpleNamespace(stdin=channel,stdout=channel)
        vm=stack.enter_context(Terminal(arch,sector,image,"seq",journal=work/"nsh.serial",qmp_bloc=b.fileno()))
        b.close()
        qmp(monitor,"qmp_capabilities")
        vm.until(b"nsh> ")
        attend(lambda:sortie(vm,"service status 16"),lambda s:"Current state: RUNNING" in s)
        def state():
            stats,=[r["stats"] for r in qmp(monitor,"query-blockstats") if r["device"]=="d"]
            return [sha(image),stats["wr_bytes"],stats["wr_operations"],stats["flush_operations"]]
        for _ in range(30):
            before=state();time.sleep(.5)
            if before==state(): break
        else: raise RuntimeError("volume actif")
        for tag,word in [("en-US","Print"),("fr-CA","Afficher"),("fr--CA","Print")]:
            vm.line("export LANG="+tag)
            for args in ["--help","-w --h","--version","--v"]:
                response=sortie(vm,"seq "+args)
                assert ("nova-cli 0.20.0" if args.startswith("--v") else word) in response
                results.append([tag,args,0])
            vm.vm.stdin.write(b"man seq; echo seqman=$?\n")
            frame=vm.until(b"q quitter" if tag=="fr-CA" else b"q quit  / search")
            assert word.encode() in frame
            vm.vm.stdin.write(b"q");vm.until(b"seqman=0");vm.until(b"nsh> ")
            results.append([tag,"man seq",0])
        after=state()
        assert before==after
        for cmd in ["seq 3","seq -s , 3","/system/bin/seq 3"]:
            sortie(vm,cmd,127)
            results.append(["nsh",cmd,127])
        vm.vm.stdin.write(b"nsh -c 'seq 3 | head -n 1'; echo seqpipe=$?\n")
        vm.until(b"seqpipe=3");vm.until(b"nsh> ")
        results.append(["nsh","seq 3 | head -n 1",3])
    return dict(media=iso_check(work/"nsh.iso",kernel,work),cas=results,avant=before,apres=after)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=TARGETS);p.add_argument("profil",choices=["sdk","nsh"])
    p.add_argument("--out",type=Path,default=Path(os.environ.get("NOVAOS_TMPDIR",HERE.parent.parent/"tmp/seq-arc027"))/"rejeu")
    p.add_argument("--vecteurs",type=Path,default=HERE/"preuves/options.jsonl.gz")
    a=p.parse_args()
    tmp=Path(os.environ.get("NOVAOS_TMPDIR",ROOT.parent/"tmp/seq-arc027")).resolve()
    tmp.mkdir(exist_ok=True,parents=True)
    os.environ.update(NOVAOS_TMPDIR=str(tmp),TMPDIR=str(tmp)+"/",NOVA_ARCH=a.arch,NOVA_RAM="256",NOVA_VCPUS="1",
        RUSTC=sp.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    work=Path(tempfile.mkdtemp(prefix=f"{a.profil}-{a.arch}-",dir=tmp))
    a.out.mkdir(parents=True,exist_ok=True)
    print(work,flush=True)
    vectors=list(rows(a.vecteurs))
    kernel=prepare(work,a.arch,vectors) if a.profil=="sdk" else None
    for sector in (512,4096):
        cell=work/str(sector);cell.mkdir()
        image,_,_=fixture(cell,sector,{"temoin":b"ARC-027\n"})
        shutil.copyfile(image,cell/"p9-initial.raw")
        result=sdk(a.arch,sector,cell,image,kernel,vectors) if kernel else shell(a.arch,sector,cell,image)
        name=f"{a.profil}-{a.arch}-{sector}"
        for ext,src in [("json",json.dumps(result,indent=2,ensure_ascii=False).encode()),
                        ("serial",(cell/f"{a.profil}.serial").read_bytes())]:
            with gzip.open(a.out/f"{name}.{ext}.gz","wb") as f:f.write(src)
        media=[cell/(a.profil+".iso"),cell/"p9-initial.raw",image]
        if (cell/"nsh.vm.json").exists():media.append(cell/"nsh.vm.json")
        archive=a.out/f"media-{name}.tar.xz"
        with tarfile.open(archive,"w:xz") as tar:
            for path in media: tar.add(path,arcname=path.name)
        with tarfile.open(archive,"r:xz") as tar:
            for path in media:
                assert hashlib.sha256(tar.extractfile(path.name).read()).hexdigest()==sha(path)
        failed=[r for r in result["cas"] if isinstance(r,dict) and not r["ok"]]
        print(name,len(result["cas"]),"écarts",len(failed),[r["id"] for r in failed[:10]],flush=True)
        if failed: raise AssertionError("écarts ELF")
if __name__=="__main__":
    main()
