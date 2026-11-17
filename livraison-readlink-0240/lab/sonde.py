"""Sonde SDK dans un invité réel, flux en mémoire et P9 en lecture seule."""
import json
import os
from pathlib import Path
import re
import select
import statistics
import subprocess
import sys
import time
from invite import ROOT, fixture, sha
from bloc_qemu import command

def main(arch,sector,out):
    out.mkdir(parents=True,exist_ok=False)
    target="aarch64-unknown-none-softfloat" if arch=="aarch64" else "x86_64-unknown-none"
    tree=Path(os.environ["TMPDIR"])/("sdk-"+arch)
    env=dict(os.environ,NOVA_ARCH=arch,CARGO_TARGET_DIR=str(tree),NOVA_RAM="256",NOVA_VCPUS="2",
        RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    cargo=["rustup","run","stable","cargo","build","--release","--locked","--offline","--target",target]
    subprocess.run([*cargo,"--bin","sonde","--features","qualification"],cwd=ROOT/"components/readlink",
        env={**env,"RUSTFLAGS":f"-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T{ROOT}/novaOS/user/novafs/service.ld"},check=True)
    elf=tree/target/"release/sonde"
    env.update(NOVA_THREAD_PROBE_ELF=str(elf),NOVA_ISO=str(out/"sdk.iso"))
    subprocess.run([*cargo,"--features","thread-probe"],cwd=ROOT/"novaOS",env=env,check=True)
    kernel=tree/target/"release/novaos"
    env["NOVA_KERNEL_ELF"]=str(kernel)
    assert elf.read_bytes() in kernel.read_bytes()
    (out/"sonde.elf").write_bytes(elf.read_bytes())
    subprocess.run(["sh",ROOT/"novaOS/scripts/iso.sh","sdk-example",env["NOVA_ISO"]],env=env,check=True)
    boot=out/"noyau-demarre.elf"
    subprocess.run(["xorriso","-osirrox","on","-indev",env["NOVA_ISO"],"-extract","/boot/novaos.elf",boot],check=True)
    assert sha(boot)==sha(kernel)
    disk,_,_=fixture(out,sector,{"temoin":b"P9\n"})
    before=sha(disk)
    cmd=command(arch,sector,disk,readonly=True,controls=["-display","none","-monitor","none","-serial","stdio"])
    records=[]
    previous=None
    ok=False
    with (out/"sdk.serial").open("wb",buffering=0) as log,subprocess.Popen(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0) as vm:
        limit=time.monotonic()+150
        buf=b""
        try:
            while time.monotonic()<limit:
                if not select.select([vm.stdout],[],[],0.5)[0]:
                    continue
                block=os.read(vm.stdout.fileno(),65536)
                stamp=time.perf_counter_ns()
                if not block:
                    break
                log.write(block)
                lines=(buf+block).split(b"\n")
                buf=lines.pop()
                for line in lines:
                    if b"READLINK MOTEUR 13 OK" in line:
                        ok=True
                        previous=stamp
                    m=re.search(rb"READLINK PERF (\d+) (\d+)",line)
                    if m:
                        kind,i=map(int,m.groups())
                        assert previous is not None and stamp>previous
                        records.append(dict(charge=kind,passage=i,chemins=4000,ns=stamp-previous))
                        previous=stamp
                    if any(marker in line for marker in (b"ASSERT [",b"Panique du noyau",b"panicked at")):
                        raise AssertionError(line)
                if b"SDK exemple admis sortie=0 restitution=OK" in b"\n".join(lines):
                    break
            else:
                raise TimeoutError(str(out))
        finally:
            vm.kill()
            vm.wait()
    assert ok and len(records)==12 and before==sha(disk)
    assert b"SDK exemple admis sortie=0 restitution=OK" in (out/"sdk.serial").read_bytes()
    medians={k:statistics.median(r["ns"] for r in records if r["charge"]==k) for k in range(4)}
    (out/"resultats.json").write_text(json.dumps(dict(architecture=arch,secteur=sector,controles=13,mesures=records,
        medianes_ns=medians,disque=before,iso=sha(out/"sdk.iso"),noyau=sha(kernel),sonde=sha(elf),
        commande=cmd),ensure_ascii=False,indent=1)+"\n")

if __name__=="__main__":
    main(sys.argv[1],int(sys.argv[2]),Path(sys.argv[3]).resolve())
