"""Recette P9, ELF de livraison dans le profil SDK, puis documentation du véritable nsh."""
import fcntl
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
from options import BASE, NOVA, run, sha

ROOT=BASE.parent
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from cli import Terminal
from journalisation import arret
from pipe_natif import fixture
from bloc_qemu import command

def blob(data):
    return "b" + '"' + "".join(chr(b) if 32<=b<127 and b not in (34,92) else f"\\x{b:02x}" for b in data) + '"'

def vectors(full):
    rows=[]
    def add(label,args,rc=0,out=b"",err=b"",port=None):
        if isinstance(out,str):out=out.encode()
        if isinstance(err,str):err=err.encode()
        rows.append(dict(test=label,argv=[os.fsencode(a).hex() for a in args],
                         statut=rc,port=(4 if err else 2) if port is None else port,sortie=(out+err).hex()))
    for args in [["-"],["-i"],["--ignore-environment"],["--","-"],["-iu","x=y"],["-i0"],
                 ["-i","--block-signal=USR1"],["-i","--default-signal"],["-i","--ignore-signal="],["-i","--list-signal-handling"]]:
        add("environnement vide",args)
    for args in [["-i","A=un","B=2","A=x"],["-S","-i A=x B=2"],["--split-string=-i A=x B=2"],
                 ["-u","A","-","A=x","B=2"],["--unset=A","--ignore-environment","A=x","B=2"]]:
        add("sortie",args,out=b"A=x\nB=2\n")
    add("guillemets",["-S",'''-i A="x y"'''],out=b"A='x y'\n")
    for args in [[],["--unset=ENV_TEST"],["-iS","A=\x24{ENV_TEST}"]]:
        add("héritage indisponible",args,125,err="env: inherited environment unavailable\n")
    for args,msg in [(["-0","x"],"cannot specify --null (-0) with command"),(["-C","x"],"must specify command with --chdir (-C)"),
                     (["-a","x"],"must specify command with --argv0 (-a)"),(["-u"],"option requires an argument -- 'u'"),
                     (["--ignore-signal=faux"],"'faux': invalid signal"),(["-z"],"invalid option -- 'z'")]:
        add("syntaxe",args,125,err=f"env: {msg}\nTry 'env --help' for more information.\n")
    for args in [["-0i","A=1"],["--null","-","A=1"]]:
        add("sortie binaire indisponible",args,125,err="env: NUL requires byte output\n")
    for args in [["--env0-from=-"],["-i","--env0-from","fictif"]]:
        add("entrée indisponible",args,125,err="env: no delegated byte input for --env0-from\n")
    for args in [["env-witness","23"],["--argv0=autre","env-witness"],["--chdir=/","env-witness"],
                 ["--ignore-signal=USR1","env-witness"],["--default-signal","env-witness"],["--list-signal-handling","env-witness"],["--block-signal=2","env-witness"]]:
        add("enfant indisponible",args,125,err="env: child launch unavailable\n")
    for opt in ["-v","--debug"]:
        add("trace",[opt,"-i"],port=4,err="clearing environment\n")
    for fr in [False,True]:
        for opt in ["--help","--version"]:
            arg=[opt,"--","nsh-doc=fr"] if fr else [opt]
            add("ARC-026",arg,out=bytes.fromhex(run(NOVA,[opt],{"LANG":"fr-CA" if fr else "en-GB"})["stdout"]))
    add("refus de sortie",["-i","A=1"],5,port=0)
    for rep in range(6 if full else 4):
        for n in [1,30]:
            entries=[f"VAR_{i:02}=fictif" for i in range(n)]
            add(f"mesure-{n}-{rep}",["-i",*entries],out="\n".join(entries)+"\n")
    return rows

def overlay(dest,rows):
    dest.mkdir()
    for name in ["components","novaFS"]:
        (dest/name).symlink_to(ROOT/name,target_is_directory=True)
    osdir=dest/"novaOS"
    osdir.mkdir()
    for p in (ROOT/"novaOS").iterdir():
        if p.name=="kernel":
            shutil.copytree(p,osdir/p.name,ignore=shutil.ignore_patterns("target","build"))
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
    (sub/"cas_env.rs").write_text("&[\n"+",\n".join(code)+"\n]")
    return osdir

def sdk_session(arch,sector,image,env,rows,directory,profile="sdk"):
    iso=directory/f"{profile}.iso"
    env=dict(env,NOVA_ISO=str(iso))
    subprocess.run(["sh",ROOT/"novaOS/scripts/iso.sh","sdk-example",iso],env=env,check=True)
    serial=directory/f"{profile}.serial"
    argv=command(arch,sector,image,readonly=True,controls=["-display","none","-monitor","none","-serial","stdio"])
    before=sha(image)
    with serial.open("wb") as log, subprocess.Popen(argv,env=env,stdout=log,stderr=subprocess.STDOUT) as proc:
        try:
            deadline=time.monotonic()+240
            while time.monotonic()<deadline:
                content=serial.read_bytes()
                if f"ENV MATRICE {len(rows)} OK".encode() in content: break
                if b"Panique du noyau" in content or proc.poll() is not None:
                    raise RuntimeError(content[-1000:])
                time.sleep(.15)
            else: raise TimeoutError(content[-1000:])
        finally:
            proc.kill()
            proc.wait()
    after=sha(image)
    assert before==after
    hz=int(re.search(rb"ENV HZ (\d+)",content)[1])
    checks=[]
    for i,row in enumerate(rows):
        match=re.search(rb"ENV BEGIN "+str(i).encode()+rb"\r?\n(.*?)ENV END "+str(i).encode()+rb" rc=(\d+) cycles=(\d+) prep=(\d+) data=(\d+) stack=(\d+)",content,re.S)
        assert match,(i,content[-700:])
        output=match[1].replace(b"\r\n",b"\n")
        # Le noyau annonce aussi certaines sorties sur la même UART.
        # Conserver la série entière et identifier uniquement la trace de collecte.
        kernel_line=rb"^PROC pid=\d+ niveau=(?:EL0|Ring3) pc=0x[0-9a-f]+ sortie="+str(row["statut"]).encode()+rb"\n"
        output=re.sub(kernel_line,b"",output,flags=re.M)
        expected=bytes.fromhex(row["sortie"])
        assert output==expected and int(match[2])==row["statut"],(i,output[:240],expected[:240])
        checks.append(dict(id=i,test=row["test"],statut=int(match[2]),compteur=int(match[3]),preparation=int(match[4]),pages_donnees_initiales=int(match[5]),pages_pile_initiales=int(match[6]),
                           stdout=output.hex() if row["port"]==2 else "",
                           stderr=output.hex() if row["port"]==4 else ""))
    perf={}
    for n in [1,30]:
        samples=[c for c in checks if c["test"].startswith(f"mesure-{n}-")][1:]
        if not samples: continue
        perf[n]={key+"_ms":statistics.median(c[key] for c in samples)*1000/hz for key in ["compteur","preparation"]}
    return dict(frequence=hz,mediane=perf,image_avant=before,image_apres=after,iso=sha(iso),commandes=checks)

def shell_session(arch,sector,image,env,directory):
    os.environ.update(env)
    serial=directory/"nsh.serial"
    (ROOT/"novaOS/build").mkdir(exist_ok=True)
    checks=[]
    with Terminal(arch,sector,image,"env",journal=serial) as vm:
        vm.until(b"nsh> ")
        def invoke(cmd,manual=False):
            vm.vm.stdin.write(cmd.encode()+b"\n")
            if manual:
                raw=vm.until(b"man env 1/")
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
            for cmd in ["env --help","env --version","man env"]:
                raw,rc=invoke(cmd,cmd.startswith("man "))
                assert rc==0
                if cmd.endswith("--version"): assert b"env (NovaOS) 0.24.0" in raw
                else: assert (b"Afficher" if locale in ("fr-CA","fr-Zzzz") else b"Display an environment") in raw,(cmd,raw[-1300:])
            after=sha(image)
            assert before==after
            checks.append(dict(locale=locale,p9_avant=before,p9_apres=after))
        assert invoke("env -i ENV_TEST=1")[1]!=0
        assert invoke("env x --help")[1]!=0
        assert invoke("env -i | wc -l")[1]!=0
        arret(vm,1)
    return dict(iso=sha(serial.with_suffix(".iso")),commandes=checks)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("out",type=Path,help="répertoire neuf, pour les deux géométries")
    p.add_argument("--mode",choices=["rapide","complet"],default="complet")
    a=p.parse_args()
    lock=(Path(os.environ["TMPDIR"])/"arc027-mesures.lock").open("a")
    fcntl.flock(lock,fcntl.LOCK_EX)
    out=a.out.resolve()
    out.mkdir(parents=True,exist_ok=False)
    tmp=Path(os.environ["TMPDIR"])/"env-arc027"
    tmp.mkdir(exist_ok=True)
    arch=a.arch
    target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[arch]
    elf=BASE/f"bin/env-{arch}.elf"
    env=dict(os.environ,NOVA_THREAD_PROBE_ELF=str(elf),NOVA_ARCH=arch,NOVA_RAM="256",NOVA_VCPUS="1",CARGO_NET_OFFLINE="true")
    cargo=["rustup","run","stable","cargo","build","--release","--offline","--locked","--target"]
    subprocess.run([*cargo,"aarch64-apple-darwin","--bin","novafs"],cwd=ROOT/"novaFS",env={**env,"CARGO_TARGET_DIR":str(ROOT/"novaFS/target")},check=True)
    witness_rows=[]
    for args,text,rc in [(["23","fictif"],"arg[0]=23\narg[1]=fictif\ncwd, argv0, environment: unavailable in startup ABI\n",23),
                         (["--moteur"],"engine bytes OK\n",0)]:
        witness_rows.append(dict(test="témoin indépendant",argv=[x.encode().hex() for x in args],statut=rc,port=2,sortie=text.encode().hex()))
    witness=BASE/f"bin/env-witness-{arch}.elf"
    rows=vectors(a.mode=="complet")
    (out/"vecteurs.json").write_text(json.dumps(rows,indent=1)+"\n")
    with tempfile.TemporaryDirectory(dir=tmp,prefix="sdk-"+arch+"-") as work:
        osdir=overlay(Path(work)/"src",rows)
        witness_dir=overlay(Path(work)/"temoin",witness_rows)
        kernels={}
        for profile,cwd,flags in [("sdk",osdir,["--features","thread-probe"]),("temoin",witness_dir,["--features","thread-probe"]),("nsh",ROOT/"novaOS",[])]:
            tree=tmp/f"kernel-{arch}-{profile}"
            subprocess.run([*cargo,target,*flags],cwd=cwd,env={**env,"CARGO_TARGET_DIR":str(tree),"NOVA_THREAD_PROBE_ELF":str(witness if profile=="temoin" else elf)},check=True)
            kernels[profile]=tree/target/"release/novaos"
        assert elf.read_bytes() in kernels["sdk"].read_bytes()
        assert witness.read_bytes() in kernels["temoin"].read_bytes()
        assert elf.read_bytes() not in kernels["nsh"].read_bytes()
        for sector in [512,4096]:
            cell=out/str(sector)
            cell.mkdir()
            image,_,_=fixture(cell,sector,{"temoin":b"P9"})
            sdk=sdk_session(arch,sector,image,dict(env,NOVA_KERNEL_ELF=str(kernels["sdk"])),rows,cell)
            temoin=sdk_session(arch,sector,image,dict(env,NOVA_KERNEL_ELF=str(kernels["temoin"])),witness_rows,cell,"temoin")
            shell=shell_session(arch,sector,image,dict(env,NOVA_KERNEL_ELF=str(kernels["nsh"])),cell)
            # Vérifier les noyaux réellement embarqués dans les deux ISO.
            for label,profile in [("sdk","sdk"),("temoin","temoin"),("nsh","nsh")]:
                extracted=Path(work)/f"{label}-{sector}.elf"
                subprocess.run(["xorriso","-osirrox","on","-indev",cell/f"{label}.iso","-extract","/boot/novaos.elf",extracted],check=True)
                assert sha(extracted)==sha(kernels[profile])
            data=dict(mode=a.mode,arch=arch,secteur=sector,elf=sha(elf),noyaux={key:sha(v) for key,v in kernels.items()},sdk=sdk,nsh=shell,temoin=temoin,elf_temoin=sha(witness))
            (cell/"resultats.json").write_text(json.dumps(data,indent=1)+"\n")
            print(arch,sector,len(sdk["commandes"]),"invocations ELF vérifiées",flush=True)
if __name__=="__main__":
    main()
