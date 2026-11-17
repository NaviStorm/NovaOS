"""Régressions de la base sur les deux architectures, sorties dans --out."""
import argparse
import fcntl
import os
import subprocess
from pathlib import Path

p=argparse.ArgumentParser(description=__doc__)
p.add_argument("--out",type=Path,required=True)
a=p.parse_args()
a.out.mkdir(parents=True,exist_ok=True)
root=Path(__file__).resolve().parents[2]
targets="test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test nsh-test nsh-scripts-test nsh-terminal-test vi-test".split()
with (a.out/"regressions.tsv").open("w",buffering=1) as summary:
    for arch in ("aarch64","x86_64"):
        for target in targets:
            with (Path(os.environ["NOVAOS_TMPDIR"])/"arc027-mesures.lock").open("a+") as lock:
                fcntl.flock(lock,fcntl.LOCK_EX)
                with (a.out/f"{arch}-{target}.log").open("w") as log:
                    result=subprocess.run(["make","-C",root/"novaOS","ARCH="+arch,target],stdout=log,stderr=subprocess.STDOUT)
            print(arch,target,result.returncode,sep="\t",file=summary)
