"""Recette P9, ELF de livraison dans le profil SDK, puis documentation du véritable nsh."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import statistics
import tempfile
import time
from options import BASE, GNU, NOVA, cases, run, sha

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
    doc_env={"LC_ALL":"en_US.UTF-8"}
    def add(label,argv,*,fr=False,denied=False):
        oracle=run(GNU,argv,doc_env)
        stdout=bytes.fromhex(oracle["stdout"])
        stderr=bytes.fromhex(oracle["stderr"])
        doc=stdout.startswith(b"Usage:") or stdout.startswith(b"basename (GNU")
        raw=stdout if not (doc or oracle["status"]) else None
        expected=b""
        code=0
        port=2
        try:
            for s in argv: s.decode()
        except UnicodeDecodeError:
            code=3
        if code:
            port=0
        elif denied:
            code,port=5,0
        elif doc:
            ref=run(NOVA,argv,{"LC_ALL":"fr-CA" if fr else "C"})
            expected=bytes.fromhex(ref["stdout"])
        elif stderr:
            code,port=3,4
            # Sur NovaOS, le diagnostic natif utilise le canal ERROR.
            expected=b"basename: "+stderr.split(b": ",1)[1].rsplit(b"\nTry ",1)[0]+b"\n"
        elif raw.endswith(b"\0"):
            code,port=25,4
            expected=b"basename: NUL unsupported on NovaOS text console\n"
        else:
            expected=bytes.fromhex(run(GNU,argv,doc_env,mode="tty")["stdout"])
        rows.append(dict(test=label,argv=[a.hex() for a in argv],brut=raw.hex() if raw is not None else None,
                         statut=code,port=port,sortie=expected.hex()))
    for label,argv in cases():
        add(label,argv)
    for argv in [[b"-a",b"d/abc",b"f"],[b"--multiple",b"",b"/",b"//",b"///",b"foo/./",b"foo/..",b"-x",b"a b",b"foo\n"],
                 [b"-za",b"",b"/",b"x\n"],[b"-s",b".txt",b"../absent/a.txt",b"/refuse/b.txt"]]:
        add("operandes-UTF8",argv)
    for opt in [b"-aa",b"--m",b"-aaz"]:
        add("alias-sur-UTF8",[opt,b"p/a",b"p/b"])
    for option in [b"--version",b"--help"]:
        add("documentation-fr", [option,b"--",b"nsh-doc=fr"],fr=True)
    add("sortie-refusee",[b"chemin/nom"],denied=True)
    for rep in range(6):
        for n in [1,30]:
            argv=[b"-s.txt",*[f"dossier-{i}/document-{i:05}.txt/".encode() for i in range(n)]]
            add(f"mesure-{n}-{rep}",argv)
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
    for name in ["options","engine"]:
        shutil.copyfile(ROOT/f"components/basename/src/{name}.rs",sub/f"{name}_basename.rs")
    code=[]
    for row in rows:
        argv=", ".join(blob(bytes.fromhex(a)) for a in row["argv"])
        raw="None" if row["brut"] is None else "Some("+blob(bytes.fromhex(row["brut"]))+")"
        code.append(f"Cas {{ argv:&[{argv}], brut:{raw}, port:{row['port']}, statut:{row['statut']} }}")
    (sub/"cas_basename.rs").write_text("&[\n"+",\n".join(code)+"\n]")
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
                if f"BASE MATRICE {len(rows)} OK".encode() in content: break
                if b"Panique du noyau" in content or proc.poll() is not None:
                    raise RuntimeError(content[-1000:])
                time.sleep(.15)
            else: raise TimeoutError(content[-1000:])
        finally:
            proc.kill()
            proc.wait()
    after=sha(image)
    assert before==after
    hz=int(re.search(rb"BASE HZ (\d+)",content)[1])
    checks=[]
    for i,row in enumerate(rows):
        match=re.search(rb"BASE BEGIN "+str(i).encode()+rb"\r?\n(.*?)BASE END "+str(i).encode()+rb" rc=(\d+) cycles=(\d+) prep=(\d+)",content,re.S)
        assert match,(i,content[-700:])
        output=match[1].replace(b"\r\n",b"\n")
        # Le noyau annonce aussi certaines sorties sur la même UART.
        # Conserver la série entière et identifier uniquement la trace de collecte.
        kernel_line=rb"^PROC pid=\d+ niveau=(?:EL0|Ring3) pc=0x[0-9a-f]+ sortie="+str(row["statut"]).encode()+rb"\n"
        output=re.sub(kernel_line,b"",output,flags=re.M)
        expected=bytes.fromhex(row["sortie"])
        assert output==expected and int(match[2])==row["statut"],(i,output[:240],expected[:240])
        checks.append(dict(id=i,test=row["test"],statut=int(match[2]),compteur=int(match[3]),preparation=int(match[4]),
                           moteur_verifie=row["brut"] is not None,stdout=output.hex() if row["port"]==2 else "",
                           stderr=output.hex() if row["port"]==4 else ""))
    perf={}
    for n in [1,30]:
        samples=[c for c in checks if c["test"].startswith(f"mesure-{n}-")][1:]
        perf[n]={key+"_ms":statistics.median(c[key] for c in samples)*1000/hz for key in ["compteur","preparation"]}
    return dict(frequence=hz,mediane=perf,image_avant=before,image_apres=after,iso=sha(iso),commandes=checks)

def shell_session(arch,sector,image,env,directory):
    os.environ.update(env)
    serial=directory/"nsh.serial"
    (ROOT/"novaOS/build").mkdir(exist_ok=True)
    checks=[]
    with Terminal(arch,sector,image,"basename",journal=serial) as vm:
        vm.until(b"nsh> ")
        def invoke(cmd,manual=False):
            vm.vm.stdin.write(cmd.encode()+b"\n")
            if manual:
                raw=vm.until(b"man basename 1/")
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
            for cmd in ["basename --help","basename --version","man basename"]:
                raw,rc=invoke(cmd,cmd.startswith("man "))
                assert rc==0
                if cmd.endswith("--version"): assert b"basename (NovaOS) 0.24.0" in raw
                else: assert (b"Afficher" if locale in ("fr-CA","fr-Zzzz") else b"Print the last") in raw,(cmd,raw[-1300:])
            after=sha(image)
            assert before==after
            checks.append(dict(locale=locale,p9_avant=before,p9_apres=after))
        assert invoke("basename -a /x/a /y/b")[1]!=0
        assert invoke("basename x --help")[1]!=0
        assert invoke("basename --erreur")[1]==3
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
    elf=BASE/f"bin/basename-{arch}.elf"
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
