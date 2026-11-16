"""Exécute l'ELF livré avec de vrais argv et zéro port, via l'admission SDK existante."""
import json
import os
from pathlib import Path
import re
import select
import subprocess
import sys
import time
from invite import ROOT, fixture, sha
from bloc_qemu import command

BASE="425e4e7cc5c03e246efb64d3638376906344bf47"
CASES=[
    ("parent",["abc/fin"],2,4),("zero",["--zero","nom/fin"],2,4),
    ("point",["simple"],2,2),("double-slash",["//"],2,2),
    ("help",["--help"],2,256),("version",["--version"],2,len("dirname (nova-dirname) 0.24.0\n")),
    ("invalide",["-h"],4,None),("manquant",[],4,None),
]

def replace_once(path,old,new):
    text=path.read_text()
    assert text.count(old)==1, path
    path.write_text(text.replace(old,new))

def prepare(tree):
    tree.mkdir(parents=True,exist_ok=False)
    paths=["components","novaOS","novaFS"]
    with subprocess.Popen(["git","archive",BASE,"--",*paths],cwd=ROOT,stdout=subprocess.PIPE) as git:
        subprocess.run(["tar","-xf","-","-C",tree],stdin=git.stdout,check=True)
        assert git.wait()==0
    # Seul le contrôleur SDK de test fournit argv. Le lanceur et sa politique
    # restent ceux de la référence. Aucun port n'est accordé au témoin SDK.
    calls=[]
    for name,args,_,_ in CASES:
        data=bytes([len(args)])+b"".join(len(v.encode()).to_bytes(2,"little")+v.encode() for v in args)
        header=bytes(32)+len(data).to_bytes(8,"little")
        calls.append(f'hal::log(format_args!("DIRNAME ELF {name}"));')
        calls.append(f'let app = spawn::launch_thread_witness(&{list(header+data)}).expect("admission SDK");')
        calls.append('assert_eq!(app.join(), Outcome::Exit(1));')
    replace_once(tree/"novaOS/kernel/src/sched/process/ipc/thread_probe.rs",
        'let app = spawn::launch_thread_witness(&[]).expect("exemple admis au catalogue");\n    assert_eq!(app.join(), Outcome::Exit(0));',
        "\n    ".join(calls))
    replace_once(tree/"novaOS/kernel/src/sched/process/ipc/thread_probe.rs","SDK exemple admis sortie=0 restitution=OK",
        "DIRNAME ELF serie terminee restitution=OK")
    # Observation du résultat de la vraie porte. Aucune lecture du tampon utilisateur.
    replace_once(tree/"novaOS/kernel/src/sched/process/syscall.rs","crate::terminal::CONSOLE => console_io(a, b, c),",
        '''crate::terminal::CONSOLE => {
            let result = console_io(a,b,c);
            hal::log(format_args!("DIRNAME CONSOLE port={a} taille={c} resultat={result:?}"));
            result
        },''')

def main(arch,sector,out,tree):
    target="aarch64-unknown-none-softfloat" if arch=="aarch64" else "x86_64-unknown-none"
    out.mkdir(parents=True,exist_ok=False)
    elf=ROOT/f"livraison-dirname-0240/bin/dirname-{arch}.elf"
    build=tree.parent/("elf-build-"+arch)
    env=dict(os.environ,NOVA_ARCH=arch,NOVA_RAM="256",NOVA_VCPUS="2",CARGO_TARGET_DIR=str(build),
        NOVA_THREAD_PROBE_ELF=str(elf),NOVA_ISO=str(out/"elf.iso"),CARGO_NET_OFFLINE="true",
        RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    subprocess.run(["rustup","run","stable","cargo","build","--release","--offline","--locked","--target",target,
        "--features","thread-probe"],cwd=tree/"novaOS",env=env,check=True)
    kernel=build/target/"release/novaos"
    env["NOVA_KERNEL_ELF"]=str(kernel)
    subprocess.run(["sh",tree/"novaOS/scripts/iso.sh","sdk-example",env["NOVA_ISO"]],env=env,check=True)
    boot=out/"noyau-demarre.elf"
    subprocess.run(["xorriso","-osirrox","on","-indev",env["NOVA_ISO"],"-extract","/boot/novaos.elf",boot],check=True)
    assert sha(boot)==sha(kernel) and elf.read_bytes() in boot.read_bytes()
    disk,_,_=fixture(out,sector,{"temoin":b"P9\n"})
    before=sha(disk)
    cmd=command(arch,sector,disk,readonly=True,controls=["-display","none","-monitor","none","-serial","stdio"])
    raw=bytearray()
    with subprocess.Popen(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0) as vm:
        deadline=time.monotonic()+70
        try:
            while b"DIRNAME ELF serie terminee restitution=OK" not in raw:
                if time.monotonic()>deadline or b"Panique du noyau" in raw:
                    raise RuntimeError(raw[-1400:])
                if select.select([vm.stdout],[],[],0.2)[0]:
                    block=os.read(vm.stdout.fileno(),32768)
                    if not block:
                        raise RuntimeError(raw[-800:])
                    raw.extend(block)
        finally:
            vm.kill()
            vm.wait()
            (out/"elf.serial").write_bytes(raw)
    assert sha(disk)==before
    rows=[]
    for name,args,port,size in CASES:
        part=raw.split(f"DIRNAME ELF {name}\r\n".encode(),1)[1].split(b"DIRNAME ELF ",1)[0]
        writes=re.findall(rb"DIRNAME CONSOLE port=(\d+) taille=(\d+) resultat=Err\(5\)",part)
        observed=[(int(p),int(s)) for p,s in writes]
        assert observed[0][0]==port and (size is None or observed[0][1]==size),(name,part)
        assert b"sortie=1" in part,(name,part)
        rows.append(dict(cas=name,argv=args,ecritures_refusees=observed,statut=1,stdout_hex="",stderr_hex=""))
    (out/"resultats.json").write_text(json.dumps(dict(arch=arch,secteur=sector,elf=sha(elf),iso=sha(out/"elf.iso"),
        noyau=sha(kernel),p9=before,base=BASE,cas=rows,commande=cmd),indent=1)+"\n")

if __name__=="__main__":
    if sys.argv[1]=="preparer":
        prepare(Path(sys.argv[2]).resolve())
    else:
        main(sys.argv[1],int(sys.argv[2]),Path(sys.argv[3]).resolve(),Path(sys.argv[4]).resolve())
