"""Recette P9, ELF de livraison dans le profil SDK, puis documentation du véritable nsh."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
from options import BASE, NOVA, invoke, LONG, SHORT
import hashlib

def sha(path):
    with path.open("rb") as stream: return hashlib.file_digest(stream,"sha256").hexdigest()


ROOT=BASE.parent
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from cli import Terminal
from journalisation import arret
from pipe_natif import fixture
from bloc_qemu import command

def blob(data):
    return "b" + '"' + "".join(chr(b) if 32<=b<127 and b not in (34,92) else f"\\x{b:02x}" for b in data) + '"'

def vectors():
    rows=[]
    def add(argv, expected=b"", status=1, port=4):
        rows.append(dict(test=" ".join(a.decode() for a in argv),argv=[a.hex() for a in argv],
                         sortie=expected.hex(),statut=status,port=port))
    for option in [b""]+[("-"+c).encode() for c in SHORT]+[("--"+s).encode() for s in LONG[:-2]]:
        argv=([option] if option else [])+[b"temoin"]
        err=b"readlink: temoin: link lookup unavailable on NovaFS P9\n" if option in (b"-v",b"--verbose") else b""
        add(argv,err)
    for argv in [[b"-nv",b"a",b"b"],[b"-vs",b"--",b"-z"],[b"-zfm",b"/../../"],[b"-x"],[]]:
        if argv[:1]==[b"-nv"]:
            expected=b"readlink: ignoring --no-newline with multiple arguments\n"+b"".join(
                b"readlink: "+p+b": link lookup unavailable on NovaFS P9\n" for p in argv[1:])
        elif not argv or argv==[b"-x"]:
            expected=invoke(NOVA,argv,{},"flux")[2]
        else: expected=b""
        add(argv,expected)
    for option in [b"--help",b"--version"]:
        for fr in [False,True]:
            args=[option]+([b"--",b"nsh-doc=fr"] if fr else [])
            rc,out,err=invoke(NOVA,args,{"LC_ALL":"fr-CA" if fr else "C"},"flux")
            assert rc==0 and not err
            add(args,out,0,2)
    return rows

def overlay(dest,rows):
    dest.mkdir()
    for name in ["components","novaFS"]:
        (dest/name).symlink_to(ROOT/name,target_is_directory=True)
    osdir=dest/"novaOS"
    osdir.mkdir()
    for p in (ROOT/"novaOS").iterdir():
        if p.name=="kernel":
            shutil.copytree(p,osdir/p.name)
        elif p.name not in ("build","target"):
            (osdir/p.name).symlink_to(p,target_is_directory=p.is_dir())
    sub=osdir/"kernel/src/sched/process/ipc"
    p=sub/"thread_probe.rs"
    source=p.read_text()
    start=source.index("pub(crate) fn run_example()")
    end=source.index("pub(crate) fn run()",start)
    p.write_text(source[:start]+(BASE/"lab/controleur.rs").read_text()+"\n"+source[end:])
    code=[]
    for row in rows:
        argv=", ".join(blob(bytes.fromhex(a)) for a in row["argv"])
        code.append(f"Cas {{ argv:&[{argv}], port:{row['port']}, statut:{row['statut']} }}")
    (sub/"cas_readlink.rs").write_text("&[\n"+",\n".join(code)+"\n]")
    return osdir

def sdk_session(arch,sector,image,env,rows,directory):
    iso=directory/"sdk.iso"
    env=dict(env,NOVA_ISO=str(iso))
    subprocess.run(["sh",ROOT/"novaOS/scripts/iso.sh","sdk-example",iso],env=env,check=True)
    serial=directory/"sdk.serial"
    argv=command(arch,sector,image,readonly=True,controls=["-display","none","-monitor","none","-serial","stdio"])
    before=sha(image)
    with serial.open("wb") as log, subprocess.Popen(argv,env=env,stdout=log,stderr=subprocess.STDOUT) as proc:
        try:
            deadline=time.monotonic()+240
            while time.monotonic()<deadline:
                content=serial.read_bytes()
                if f"READLINK MATRICE {len(rows)} OK".encode() in content: break
                if b"Panique du noyau" in content or proc.poll() is not None:
                    raise RuntimeError(content[-1000:])
                time.sleep(.15)
            else: raise TimeoutError(content[-1000:])
        finally:
            proc.kill()
            proc.wait()
    after=sha(image)
    assert before==after
    hz=int(re.search(rb"READLINK HZ (\d+)",content)[1])
    checks=[]
    for i,row in enumerate(rows):
        match=re.search(rb"READLINK BEGIN "+str(i).encode()+rb"\r?\n(.*?)READLINK END "+str(i).encode()+rb" rc=(\d+) cycles=(\d+) prep=(\d+)",content,re.S)
        assert match,(i,content[-700:])
        output=match[1].replace(b"\r\n",b"\n")
        # Le noyau annonce aussi certaines sorties sur la même UART.
        # Conserver la série entière et identifier uniquement la trace de collecte.
        kernel_line=rb"^PROC pid=\d+ niveau=(?:EL0|Ring3) pc=0x[0-9a-f]+ sortie="+str(row["statut"]).encode()+rb"\n"
        output=re.sub(kernel_line,b"",output,flags=re.M)
        expected=bytes.fromhex(row["sortie"])
        assert output==expected and int(match[2])==row["statut"],(i,output[:240],expected[:240])
        checks.append(dict(id=i,test=row["test"],statut=int(match[2]),compteur=int(match[3]),preparation=int(match[4]),
                           stdout=output.hex() if row["port"]==2 else "",
                           stderr=output.hex() if row["port"]==4 else ""))
    return dict(frequence=hz,image_avant=before,image_apres=after,iso=sha(iso),commandes=checks)

def shell_session(arch,sector,image,env,directory):
    os.environ.update(env)
    serial=directory/"nsh.serial"
    (ROOT/"novaOS/build").mkdir(exist_ok=True)
    checks=[]
    with Terminal(arch,sector,image,"readlink",journal=serial) as vm:
        vm.until(b"nsh> ")
        def invoke(cmd,manual=False):
            vm.vm.stdin.write(cmd.encode()+b"\n")
            if manual:
                raw=vm.until(b"man readlink 1/")
                vm.vm.stdin.write(b"q")
            else: raw=b""
            raw+=vm.until(b"nsh> " if manual else b"\r\nnsh> ")
            vm.vm.stdin.write(b"echo BB_$?__fin\n")
            status=vm.until(b"__fin\r\n")
            if not re.search(rb"BB_(\d+)__fin",status):status+=vm.until(b"__fin\r\n")
            rc=int(re.search(rb"BB_(\d+)__fin",status)[1])
            vm.until(b"nsh> ")
            checks.append(dict(commande=cmd,statut=rc,sortie=raw.hex()))
            return raw,rc
        # Le journal de démarrage peut encore écrire après la première invite.
        for _ in range(15):
            before=sha(image)
            time.sleep(2)
            if sha(image)==before: break
        else: raise TimeoutError("P9 actif au repos")
        for locale in ["fr-CA","fr-Zzzz","fr-zzzzz","en-GB"]:
            assert invoke("export LANG="+locale)[1]==0
            time.sleep(.3)
            before=sha(image)
            for cmd in ["readlink --help","readlink --version","man readlink"]:
                raw,rc=invoke(cmd,cmd.startswith("man "))
                assert rc==0
                if cmd.endswith("--version"): assert b"readlink (nova-readlink) 0.24.0" in raw
                else: assert (b"Afficher" if locale in ("fr-CA","fr-Zzzz") else b"Print a link target") in raw,(cmd,raw[-1300:])
            after=sha(image)
            assert before==after
            checks.append(dict(locale=locale,p9_avant=before,p9_apres=after))
        assert invoke("readlink /temoin")[1]==127
        assert invoke("readlink -m /absent")[1]==127
        invoke("readlink /temoin | wc")
        assert invoke("readlink --erreur")[1]==1
        arret(vm,1)
    return dict(iso=sha(serial.with_suffix(".iso")),commandes=checks)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("out",type=Path,help="répertoire neuf, pour les deux géométries")
    a=p.parse_args()
    out=a.out.resolve()
    out.mkdir(parents=True,exist_ok=False)
    tmp=Path(os.environ["TMPDIR"])
    arch=a.arch
    target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[arch]
    elf=BASE/f"bin/readlink-{arch}.elf"
    env=dict(os.environ,NOVA_THREAD_PROBE_ELF=str(elf),NOVA_ARCH=arch,NOVA_RAM="256",NOVA_VCPUS="1",CARGO_NET_OFFLINE="true")
    cargo=["rustup","run","stable","cargo","build","--release","--offline","--locked","--target"]
    subprocess.run([*cargo,"aarch64-apple-darwin","--bin","novafs"],cwd=ROOT/"novaFS",env={**env,"CARGO_TARGET_DIR":str(ROOT/"novaFS/target")},check=True)
    rows=vectors()
    (out/"vecteurs.json").write_text(json.dumps(rows,indent=1)+"\n")
    with tempfile.TemporaryDirectory(dir=tmp,prefix="sdk-"+arch+"-") as work:
        osdir=overlay(Path(work)/"src",rows)
        kernels={}
        for profile,cwd,flags in [("sdk",osdir,["--features","thread-probe"]),("nsh",ROOT/"novaOS",[])]:
            tree=tmp/f"kernel-{arch}-{profile}"
            subprocess.run([*cargo,target,*flags],cwd=cwd,env={**env,"CARGO_TARGET_DIR":str(tree)},check=True)
            kernels[profile]=tree/target/"release/novaos"
        assert elf.read_bytes() in kernels["sdk"].read_bytes()
        assert elf.read_bytes() not in kernels["nsh"].read_bytes()
        for sector in [512,4096]:
            cell=out/str(sector)
            cell.mkdir()
            image,_,_=fixture(cell,sector,{"temoin":b"P9"})
            sdk=sdk_session(arch,sector,image,dict(env,NOVA_KERNEL_ELF=str(kernels["sdk"])),rows,cell)
            shell=shell_session(arch,sector,image,dict(env,NOVA_KERNEL_ELF=str(kernels["nsh"])),cell)
            # Vérifier les noyaux réellement embarqués dans les deux ISO.
            for label,profile in [("sdk","sdk"),("nsh","nsh")]:
                extracted=Path(work)/f"{label}-{sector}.elf"
                subprocess.run(["xorriso","-osirrox","on","-indev",cell/f"{label}.iso","-extract","/boot/novaos.elf",extracted],check=True)
                assert sha(extracted)==sha(kernels[profile])
            data=dict(arch=arch,secteur=sector,elf=sha(elf),noyaux={key:sha(v) for key,v in kernels.items()},sdk=sdk,nsh=shell)
            (cell/"resultats.json").write_text(json.dumps(data,indent=1)+"\n")
            print(arch,sector,len(sdk["commandes"]),"invocations ELF vérifiées",flush=True)
if __name__=="__main__":main()
