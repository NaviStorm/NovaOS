"""Régressions des deux chemins nsh touchés par le catalogue de documentation."""
import fcntl
import os
from pathlib import Path
import shutil
import subprocess
import sys

root=Path(__file__).resolve().parents[2]
out=Path(sys.argv[1]).resolve()
out.mkdir(parents=True,exist_ok=True)
with (Path(os.environ["NOVAOS_TMPDIR"])/"arc027-mesures.lock").open("a") as lock:
    fcntl.flock(lock,fcntl.LOCK_EX)
    for arch,target in [("aarch64","aarch64-unknown-none-softfloat"),("x86_64","x86_64-unknown-none")]:
        build=Path(os.environ["TMPDIR"])/("kernel-"+arch)
        env=dict(os.environ,NOVA_ARCH=arch,NOVA_VCPUS="2",NOVA_RAM="256",CARGO_TARGET_DIR=str(build),
                 NOVA_KERNEL_ELF=str(build/target/"release/novaos"))
        with (out/f"{arch}.log").open("wb") as log:
            subprocess.run(["make","kernel","ARCH="+arch],cwd=root/"novaOS",env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
            for script,prefix in [("nsh.py","nsh"),("nsh_scripts.py","scripts")]:
                subprocess.run([sys.executable,"-B","tests/"+script],cwd=root/"novaOS",env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
                for sector in [512,4096]:
                    shutil.copy(root/"novaOS/build"/f"{prefix}-{arch}-{sector}.json",out)
