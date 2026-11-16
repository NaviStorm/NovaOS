"""Exécuter les vecteurs du moteur en EL0 ou Ring 3, dans l'emplacement SDK de qualification."""
import argparse
import json
import os
import shutil
import subprocess
import time
import sys
from pathlib import Path
from laboratoire import HERE,TMP,digest
root=HERE.parent
sys.path.insert(0,str(root/"novaOS/tests"))
from threads import TARGETS
from demarrage import boot

p=argparse.ArgumentParser(description=__doc__)
p.add_argument("arch",choices=TARGETS)
p.add_argument("out",type=Path)
a=p.parse_args()
out=a.out.resolve()
out.mkdir(parents=True,exist_ok=False)
target=TARGETS[a.arch]
env={**os.environ,"TMPDIR":str(TMP)+"/","NOVAOS_TMPDIR":str(TMP),
    "RUSTC":subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip(),
    "CARGO_TARGET_DIR":str(TMP/"cmp-sdk"),"CMP_VECTEURS":str(HERE/"preuves/vecteurs.bin")}
def build(cwd,args,**extra):
    subprocess.run(["rustup","run","stable","cargo","build","--release","--offline","--locked",
        "--target",target,*args],cwd=cwd,env={**env,**extra},check=True)
build(root/"components/cmp",["--features","temoin"],RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T"+str(root/"novaOS/user/novafs/service.ld"))
elf=out/"cmp-temoin.elf"
shutil.copy2(TMP/"cmp-sdk"/target/"release/cmp",elf)
build(root/"novaOS",["--features","thread-probe"],NOVA_THREAD_PROBE_ELF=str(elf))
kernel=TMP/"cmp-sdk"/target/"release/novaos"
assert elf.read_bytes() in kernel.read_bytes()
os.environ.update(NOVA_ARCH=a.arch,NOVA_KERNEL_ELF=str(kernel),NOVAOS_TMPDIR=str(TMP),TMPDIR=str(TMP)+"/")
iso=out/"sdk.iso"
subprocess.run(["sh",root/"novaOS/scripts/iso.sh","sdk-example",iso],check=True)
extracted=out/"noyau.elf"
subprocess.run(["xorriso","-osirrox","on","-indev",iso,"-extract","/boot/novaos.elf",extracted],check=True)
assert digest(extracted)==digest(kernel)
def check(vm,directory,log):
    end=time.monotonic()+60
    count=len(json.loads((HERE/"preuves/vecteurs.json").read_text()))
    while time.monotonic()<end and vm.poll() is None:
        trace=log.read_bytes() if log.exists() else b""
        if b"Panique du noyau" in trace:raise RuntimeError(trace[-1200:])
        if b"SDK exemple admis sortie=0 restitution=OK" in trace:
            assert f"CMP vecteurs={count} ecarts=0".encode() in trace
            return dict(cas=count,sortie=0,restitution=True)
        time.sleep(0.1)
    raise RuntimeError("témoignage SDK incomplet")
label="cmp-sdk-"+a.arch
result=boot(label,check,NOVA_ISO=str(iso),NOVA_VCPUS="2")
log=root/"novaOS/build"/("serie-"+label+".log")
shutil.copy2(log,out/"serie.log")
assert f"ecarts=0" in log.read_text()
(out/"resultat.json").write_text(json.dumps(dict(resultat=result,iso=digest(iso),noyau=digest(kernel),elf=digest(elf),
    vecteurs=digest(HERE/"preuves/vecteurs.bin")),indent=2)+"\n")
