"""Exécuter les vecteurs du moteur en EL0 ou Ring 3, dans l'emplacement SDK de qualification."""
import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from laboratoire import HERE,TMP,digest
root=HERE.parent
sys.path.insert(0,str(root/"novaOS/tests"))
from threads import TARGETS,exercise_example

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
result=exercise_example(a.arch,2)
log=root/"novaOS/build"/f"serie-{a.arch}-sdk-example-2cpu.log"
shutil.copy2(log,out/"serie.log")
assert f"ecarts=0" in log.read_text()
(out/"resultat.json").write_text(json.dumps(dict(resultat=result,noyau=digest(kernel),elf=digest(elf),
    vecteurs=digest(HERE/"preuves/vecteurs.bin")),indent=2)+"\n")
