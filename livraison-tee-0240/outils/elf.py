"""Qualification de l'ELF de livraison inchangé via le point d'admission du SDK."""
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import subprocess as sp
import sys
import time

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/"novaOS/scripts"))
from bloc_qemu import command

def run(arch,sector,out):
    tmp=Path(os.environ["TMPDIR"])
    lab=tmp/("sdk-"+arch)
    target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[arch]
    lab.mkdir(exist_ok=True)
    # Le lanceur, l'ABI et le service de stockage sont ceux du dépôt.
    for name in ("novaOS","components","novaFS"):
        shutil.copytree(ROOT/name,lab/name,dirs_exist_ok=True,ignore=shutil.ignore_patterns("target","build","__pycache__"))
    recipe=lab/"novaOS/kernel/src/sched/process/ipc/thread_probe.rs"
    s=recipe.read_text()
    start=s.index("    let app = spawn::launch_thread_witness(&[])",s.index("pub(crate) fn run_example()"))
    end=s.index("    assert!(with(",start)
    corpus=ROOT/"livraison-tee-0240/preuves/macOS.json"
    actions={}
    for case in json.loads(corpus.read_text())["rows"]:
        words=tuple(case["args"])
        if case["posix"] or any(not word.isascii() for word in words): continue
        actions[words]={"documentation":0,"invalide":1,"copie":5}[case["action"]]
    for option in ("--help","--version"):
        actions[(option,"--","nsh-doc=fr")]=0
    vectors=",\n".join("(&["+ ",".join(json.dumps(w) for w in words)+"],"+str(rc)+")" for words,rc in actions.items())
    body=Path(__file__).with_name("recette-sdk.rs").read_text().replace("/* VECTEURS */",vectors)
    recipe.write_text(s[:start]+body+s[end:])
    binary=ROOT/("livraison-tee-0240/bin/tee-"+arch+".elf")
    kernel=lab/"target"/target/"release/novaos"
    iso=out/"elf.iso"
    env=dict(os.environ,NOVA_ARCH=arch,NOVA_VCPUS="2",NOVA_RAM="256",CARGO_TARGET_DIR=str(lab/"target"),
             NOVA_THREAD_PROBE_ELF=str(binary),NOVA_KERNEL_ELF=str(kernel),NOVA_ISO=str(iso),
             RUSTC=sp.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    sp.run(["rustup","run","stable","cargo","build","--offline","--locked","--release","-p","novaos","--features","thread-probe","--target",target],cwd=lab/"novaOS",env=env,check=True)
    sp.run(["sh",ROOT/"novaOS/scripts/iso.sh","sdk-example",iso],env=env,check=True)
    extracted=out/"sdk-kernel.elf"
    sp.run(["xorriso","-osirrox","on","-indev",iso,"-extract","/boot/novaos.elf",extracted],check=True)
    assert extracted.read_bytes()==kernel.read_bytes() and binary.read_bytes() in extracted.read_bytes()
    image=out/"session.raw"
    h=lambda:hashlib.sha256(image.read_bytes()).hexdigest()
    before=h()
    argv=command(arch,sector,image,readonly=True,controls=["-display","none","-monitor","none","-serial","stdio"])
    p=sp.Popen(argv,stdout=sp.PIPE,stderr=sp.STDOUT,stdin=sp.DEVNULL,env=env,bufsize=0)
    data=bytearray()
    try:
        end=time.monotonic()+180
        while time.monotonic()<end:
            if not select.select([p.stdout],[],[],1)[0]:continue
            chunk=os.read(p.stdout.fileno(),65536)
            if not chunk:break
            data.extend(chunk)
            if b"SDK exemple admis sortie=0 restitution=OK" in data or b"Panique du noyau" in data:break
    finally:
        p.kill();p.wait()
        (out/"elf.serial").write_bytes(data)
    assert b"SDK exemple admis sortie=0 restitution=OK" in data and data.count(b"TEE-ELF")==len(actions),data[-1200:]
    assert all(word in data for word in [b"Copy input",b"Copier",b"nova-cli 0.20.0",b"unrecognized option"])
    assert before==h()
    (out/"elf.json").write_text(json.dumps(dict(elf=hashlib.sha256(binary.read_bytes()).hexdigest(),
        avant=before,apres=h(),cas=[dict(argv=k,rc=v) for k,v in actions.items()]),indent=2)+"\n")
    print(arch,sector,"ELF exact,",len(actions),"contrôles")
if __name__=="__main__":run(sys.argv[1],int(sys.argv[2]),Path(sys.argv[3]).resolve())
