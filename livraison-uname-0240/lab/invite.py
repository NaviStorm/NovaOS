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

from options import HERE, NOVA, ENV, invoke
import tomllib
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


def guest_output(raw, arch, release):
    # GNU a résolu les options. Seules les valeurs vérifiées dans le noyau invité changent.
    fields = {b"KERNEL":b"NovaOS", b"NODE":b"unknown", b"RELEASE":release,
              b"VERSION":b"NovaOS v"+release, b"MACHINE":arch.encode(), b"arm":b"unknown", b"Darwin":b"NovaOS"}
    if raw.startswith(b"Kernel name: "):
        return b"".join(label+b": "+fields[val]+b"\n" for line in raw.splitlines()
                        for label,val in [line.split(b": ",1)] if label!=b"Processor")
    return b" ".join(fields.get(val,val) for val in raw.rstrip(b"\n").split(b" "))+b"\n"


def rows(path,arch,release):
    seen=set()
    for idx,row in enumerate(json.loads(path.read_text())):
        assert row["ok"]
        args=tuple(map(bytes.fromhex,row["argv"]))
        if (args in seen or row["mode"]!="normal" or row["env"]!=ENV):
            continue
        seen.add(args)
        if any(not x.isascii() for x in args):
            continue  # Les frontières UTF-8 et les diagnostics Unicode ont des vecteurs dédiés.
        code,out,err=row["gnu"];out=bytes.fromhex(out);err=bytes.fromhex(err).replace(b"guname:",b"uname:",1)
        if row["doc"]:
            rc,out,err=invoke(NOVA,args)
            assert rc==0
        elif code==0:
            out=guest_output(out,arch,release)
            # L’ensemble GNU omet le champ processeur absent sur cette cible.
            if any((b"a" in x[1:] or b"A" in x[1:]) and x.startswith(b"-") and not x.startswith(b"--")
                   or x.startswith(b"--") and x[2:] and (b"all".startswith(x[2:]) or b"all-labeled".startswith(x[2:])) for x in args):
                if not out.startswith(b"Kernel name: "):
                    out=out.replace(arch.encode()+b" unknown NovaOS",arch.encode()+b" NovaOS")
        yield f"darwin-{idx}",list(args),6,(code,out,err),"GNU + identité du noyau"
    yield "défaut",[],6,(0,b"NovaOS\n",b""),"bannière"
    yield "utf8-invalide",[b"\xff"],6,(1,b"", "uname: argument invalide\n".encode()),"décodage argv"
    yield "utf8-valide",["é".encode()],6,(1,b"",b"uname: extra operand '\\303\\251'\nTry 'uname --help' for more information.\n"),"diagnostic"
    for opt in ["--help","--version"]:
        yield "doc-fr-"+opt,[opt.encode(),b"--",b"nsh-doc=fr"],6,invoke(NOVA,[opt],{"LC_ALL":"fr-CA"}),"ARC-026 FR"
    yield "sortie refusée",[b"-s"],4,(1,b"","uname: accès refusé\n".encode()),"droit écriture"
    yield "aucun port",[b"-s"],0,(1,b"",b""),"droits des deux flux"


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
    source.with_name("uname-argv.bin").write_bytes(buf)
    p=kernel/"kernel/src/sched/process/syscall.rs"
    text=p.read_text()
    anchor="    LINE_OPEN.store(!text.ends_with('\\n'), Ordering::Relaxed);"
    assert text.count(anchor)==1
    p.write_text(text.replace(anchor,'''    if ipc::thread_probe::TRACER.load(Ordering::Relaxed) { for octets in bytes.chunks(32) {
        hal::log(format_args!("UNAMEHEX {} {right} {:02x?}",pid.number,octets));
    } }
'''+anchor))
    elf=HERE/f"bin/uname-{arch}.elf"
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


def sdk(arch,sector,work,image,kernel,vectors,release):
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
                if f"UNAMETERM {len(vectors)}".encode() in text: break
            else: raise TimeoutError("recette SDK incomplète")
        finally: vm.kill();vm.wait()
    log=log.read_bytes()
    results=[]
    for i,(key,args,ports,expected,mode) in enumerate(vectors):
        start=re.search(rb"UNAMEBEGIN "+str(i).encode()+rb" pid=(\d+) ports=(\d+)",log)
        finish=re.search(rb"UNAMEEND "+str(i).encode()+rb" Exit\((\d+)\)",log)
        assert start and finish and int(start[2])==ports,(i,key,log[-400:])
        frames=re.findall(rb"UNAMEHEX (\d+) ([24]) \[([0-9a-f, ]+)\]",log[start.end():finish.start()])
        out,err=bytearray(),bytearray()
        for pid,port,data in frames:
            assert pid==start[1]
            (out if port==b"2" else err).extend(bytes.fromhex(data.decode().replace(",","")))
        got=int(finish[1]),out,err
        result=dict(id=key,argv=[x.hex() for x in args],ports=ports,pid=int(start[1]),oracle=mode,ok=got==expected,
                    statut=got[0],stdout=base64.b64encode(out).decode(),stderr=base64.b64encode(err).decode())
        results.append(result)
    banner=re.search(rb"(?m)^NovaOS v([0-9.]+)\r?$",log)
    assert banner and banner[1]==release, (banner,release)
    perf=[]
    for arg,expected in [("-s",b"NovaOS"),("-a",b"NovaOS unknown "+release+b" NovaOS v"+release+b" "+arch.encode()+b" NovaOS"),
                         ("-omrs",b"NovaOS "+release+b" "+arch.encode()+b" NovaOS")]:
        for count in (1,8):
            measures=[]
            ticks=[]
            for turn in range(4):
                trial=re.search(fr"UNAMEBENCH {arg} {count} {turn}\r\n(.*?)UNAMETIME (\d+) (\d+) {count} (\d+)\r\n".encode(),log,re.S)
                assert trial and trial[1].replace(b"\r\n",b"\n")==(expected+b"\n")*count,(arg,count,turn,trial)
                measures.append(int(trial[2])/int(trial[3]))
                ticks.append(dict(total=int(trial[2]),hz=int(trial[3]),admission=int(trial[4])))
            perf.append(dict(arg=arg,appels=count,compteurs=ticks,secondes=measures[1:],echauffement=measures[0],
                             mediane=statistics.median(measures[1:]),octets_par_appel=len(expected)+1))
    assert before==sha(image)
    return dict(media=media,elf=sha(HERE/f"bin/uname-{arch}.elf"),p9=before,commande=argv,cas=results,banniere=banner[0].strip().decode(),performance=perf)


def shell(arch,sector,work,image):
    kernel=OS/"target"/TARGETS[arch]/"release/novaos"
    os.environ["NOVA_KERNEL_ELF"]=str(kernel)
    results=[]
    with ExitStack() as stack:
        a,b=socket.socketpair()
        stack.enter_context(a);stack.enter_context(b)
        channel=stack.enter_context(a.makefile("rwb",buffering=0))
        monitor=SimpleNamespace(stdin=channel,stdout=channel)
        vm=stack.enter_context(Terminal(arch,sector,image,"uname",journal=work/"nsh.serial",qmp_bloc=b.fileno()))
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
            for args in ["--help","-A --he","--version","--v"]:
                response=sortie(vm,"uname "+args)
                assert ("uname (nova-uname) 0.24.0" if args.startswith("--v") else word) in response
                results.append([tag,args,0])
            vm.vm.stdin.write(b"man uname; echo unameman=$?\n")
            frame=vm.until(b"q quitter" if tag=="fr-CA" else b"q quit  / search")
            assert word.encode() in frame
            vm.vm.stdin.write(b"q");vm.until(b"unameman=0");vm.until(b"nsh> ")
            results.append([tag,"man uname",0])
        after=state()
        assert before==after
        for cmd in ["uname","uname -a","/system/bin/uname -s"]:
            sortie(vm,cmd,127)
            results.append(["nsh",cmd,127])
        vm.vm.stdin.write(b"uname -r; echo direct=$?\n")
        vm.until(b"direct=127\r\n");vm.until(b"nsh> ")
        results.append(["terminal","uname -r",127])
        vm.vm.stdin.write(b"nsh -c 'uname -a | head -n 1'; echo unamepipe=$?\n")
        vm.until(b"unamepipe=3");vm.until(b"nsh> ")
        results.append(["nsh","uname -a | head -n 1",3])
    return dict(media=iso_check(work/"nsh.iso",kernel,work),cas=results,avant=before,apres=after)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=TARGETS);p.add_argument("profil",choices=["sdk","nsh"])
    p.add_argument("--out",type=Path,default=Path(os.environ.get("NOVAOS_TMPDIR",HERE.parent.parent/"tmp/uname-arc027"))/"rejeu")
    p.add_argument("--vecteurs",type=Path,default=HERE/"preuves/options.json")
    a=p.parse_args()
    tmp=Path(os.environ.get("TMPDIR",ROOT.parent/"tmp/uname-arc027")).resolve()
    tmp.mkdir(exist_ok=True,parents=True)
    os.environ.update(TMPDIR=str(tmp)+"/",NOVA_ARCH=a.arch,NOVA_RAM="256",NOVA_VCPUS="1",
        RUSTC=sp.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    work=Path(tempfile.mkdtemp(prefix=f"{a.profil}-{a.arch}-",dir=tmp))
    a.out.mkdir(parents=True,exist_ok=True)
    print(work,flush=True)
    release=tomllib.loads((OS/"kernel/Cargo.toml").read_text())["package"]["version"].encode()
    vectors=list(rows(a.vecteurs,a.arch,release)) if a.profil=="sdk" else []
    kernel=prepare(work,a.arch,vectors) if a.profil=="sdk" else None
    for sector in (512,4096):
        cell=work/str(sector);cell.mkdir()
        image,_,_=fixture(cell,sector,{"temoin":b"ARC-027\n"})
        shutil.copyfile(image,cell/"p9-initial.raw")
        if kernel:
            result=sdk(a.arch,sector,cell,image,kernel,vectors,release)
        else:
            result=shell(a.arch,sector,cell,image)
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
