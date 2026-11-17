"""Sonde dans le travailleur admis par TMP_WORKER, volume P9 jetable."""
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import time
from elf import BASE, replace_once
from invite import ROOT, fixture, sha
from bloc_qemu import command

def prepare(tree, commande=False,duo=False):
    tree.mkdir(parents=True,exist_ok=False)
    with subprocess.Popen(["git","archive",BASE,"components","novaOS","novaFS"],cwd=ROOT,stdout=subprocess.PIPE) as git:
        subprocess.run(["tar","-xf","-","-C",tree],stdin=git.stdout,check=True)
        assert git.wait()==0
    p=tree/"novaOS/kernel/src/sched/process/ipc/tmp_probe.rs"
    s=p.read_text()
    p.write_text(s[:s.index("fn word(")]+'''pub(crate) fn run(_: &[u8]) {
        await_condition(super::super::tmp::ready_shell);
        assert_eq!(launch(1).join(), Outcome::Exit(0));
        hal::log(format_args!("MKTEMP demandeur termine OK"));
    }
''')
    replace_once(tree/"novaOS/kernel/src/sched/process/ipc/spawn.rs",
                 'include_bytes!(concat!(env!("OUT_DIR"), "/tmp_sample.bin"))',
                 'include_bytes!(env!("MKTEMP_SONDE"))')

    if commande:
        replace_once(tree/"novaOS/kernel/src/sched/process/ipc/spawn.rs",
                     "p.worker_image = workers.then_some(self.0);",
                     'p.worker_image = workers.then_some(if self.1 == iw::SAMPLE {include_bytes!(env!("MKTEMP_ELF"))} else {self.0});')

    if duo:
        replace_once(p,"assert_eq!(launch(1).join(), Outcome::Exit(0));",
            "let a=launch(2); let b=launch(2); assert_eq!(a.join(),Outcome::Exit(0)); assert_eq!(b.join(),Outcome::Exit(0));")

def main(arch,sector,out,tree,commande=False,duo=False):
    out.mkdir(parents=True,exist_ok=False)
    target="aarch64-unknown-none-softfloat" if arch=="aarch64" else "x86_64-unknown-none"
    env=dict(os.environ,NOVA_ARCH=arch,NOVA_RAM="256",NOVA_VCPUS="2",NOVA_ISO=str(out/"tmp.iso"),
        CARGO_TARGET_DIR=str(tree.parent/(tree.name+"-build-"+arch)),CARGO_NET_OFFLINE="true",
        RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    from vecteurs import generate
    vectors=out/"vecteurs.rs"
    scope=generate(ROOT/"livraison-mktemp-0240/preuves/options.json",vectors) if commande else None
    env["MKTEMP_VECTEURS"]=str(vectors)
    binary="demandeur" if commande else "sonde"
    env["MKTEMP_ELF"]=str(ROOT/f"livraison-mktemp-0240/bin/mktemp-{arch}.elf")
    cargo=["rustup","run","stable","cargo","build","--release","--offline","--locked","--target",target]
    subprocess.run([*cargo,"--features","qualification","--bin",binary],cwd=ROOT/"components/mktemp",
        env=dict(env,RUSTFLAGS=f"-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T{ROOT}/novaOS/user/novafs/service.ld"),check=True)
    probe=Path(env["CARGO_TARGET_DIR"])/target/"release"/binary
    env["MKTEMP_SONDE"]=str(probe)
    subprocess.run(cargo,cwd=tree/"novaOS",env=env,check=True)
    kernel=probe.with_name("novaos")
    assert probe.read_bytes() in kernel.read_bytes()
    if commande: assert Path(env["MKTEMP_ELF"]).read_bytes() in kernel.read_bytes()
    env["NOVA_KERNEL_ELF"]=str(kernel)
    subprocess.run(["sh",tree/"novaOS/scripts/iso.sh","tmp-test",env["NOVA_ISO"]],env=env,check=True)
    boot=out/"noyau-demarre.elf"
    subprocess.run(["xorriso","-osirrox","on","-indev",env["NOVA_ISO"],"-extract","/boot/novaos.elf",boot],check=True)
    assert sha(boot)==sha(kernel)
    disk,_,_=fixture(out,sector,{"temoin":b"ARC027\n"})
    before=sha(disk)
    cmd=command(arch,sector,disk,controls=["-display","none","-monitor","none","-serial","stdio"])
    stamps={}
    import select
    with subprocess.Popen(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,stdin=subprocess.PIPE,bufsize=0) as vm,(out/"tmp.serial").open("xb",buffering=0) as log:
        deadline=time.monotonic()+180
        pending=b""
        try:
            while time.monotonic()<deadline:
                if not select.select([vm.stdout],[],[],0.25)[0]: continue
                chunk=os.read(vm.stdout.fileno(),65536)
                if not chunk: raise RuntimeError("fin de la VM")
                stamp=time.monotonic_ns()
                log.write(chunk)
                lines=(pending+chunk).split(b"\n")
                pending=lines.pop()
                for line in lines:
                    match=re.search(rb"DEBUG pid=(\d+) MKTEMP (DEBUT|FIN) ([1-3])",line)
                    if match: stamps[int(match[1]),match[2],int(match[3])]=stamp
                    if b"Panique du noyau" in line or b"panicked at" in line: raise RuntimeError(line)
                    if b"MKTEMP demandeur termine OK" in line: vm.stdin.write(b"exit\n")
                if any(b"SMP arret motif=normal" in line for line in lines): break
            else: raise TimeoutError(out)
        finally: vm.kill();vm.wait()
    serial=(out/"tmp.serial").read_bytes()
    assert b"MKTEMP RELECTURE OK" in serial and b"BLOC session terminee ok=true" in serial
    parents=sorted({k[0] for k in stamps})
    assert len(parents)==(2 if duo else 1)
    ns=[]
    spans=[]
    for p in (1,2,3):
        starts=[stamps[pid,b"DEBUT",p] for pid in parents]
        ends=[stamps[pid,b"FIN",p] for pid in parents]
        assert max(starts)<min(ends), "lots concurrents sans recouvrement"
        ns.append(max(ends)-min(starts))
        spans.append(list(zip(starts,ends)))
    peak=int(re.search(rb"BLOC memoire .*?pic_cadres=(\d+)",serial)[1])
    count=8*len(parents)
    rows=dict(matrice=scope if not duo else None,arch=arch,secteur=sector,ns=ns,objets_par_passage=count,parents=parents,intervalles=spans,pic_cadres_session=peak,collisions_forcees_par_passage=0 if commande else 8,elf=sha(Path(env["MKTEMP_ELF"])) if commande else None,
              creations_s=count*1e9/statistics.median(ns),noyau=sha(boot),sonde=sha(probe),iso=sha(out/"tmp.iso"),
              avant=before,apres=sha(disk),commande=cmd,qemu=subprocess.check_output([f"qemu-system-{arch}","--version"],text=True).splitlines()[0])
    (out/"resultats.json").write_text(json.dumps(rows,indent=2)+"\n")

if __name__=="__main__":
    if sys.argv[1]=="preparer": prepare(Path(sys.argv[2]).resolve(),any(m in sys.argv for m in ["--commande","--duo"]),"--duo" in sys.argv)
    else: main(sys.argv[1],int(sys.argv[2]),Path(sys.argv[3]).resolve(),Path(sys.argv[4]).resolve(),any(m in sys.argv for m in ["--commande","--duo"]),"--duo" in sys.argv)
