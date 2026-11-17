"""Rejouer les trois bancs du shell avec les noyaux ordinaires de la recette."""
import argparse
import fcntl
import json
import os
import shutil
import time
from pathlib import Path
import subprocess
import sys
from options import ROOT, sha

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("recette",type=Path)
    p.add_argument("out",type=Path)
    a=p.parse_args()
    tmp=Path(os.environ["TMPDIR"])
    target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[a.arch]
    kernel=tmp/"env-arc027"/f"kernel-{a.arch}-nsh"/target/"release/novaos"
    proofs=json.loads((a.recette/"512/resultats.json").read_text())
    assert sha(kernel)==proofs["noyaux"]["nsh"]
    a.out.mkdir(exist_ok=False,parents=True)
    env=dict(os.environ,NOVA_ARCH=a.arch,NOVA_KERNEL_ELF=str(kernel),NOVA_VCPUS="1",NOVA_RAM="256")
    results={}
    with (tmp/"arc027-mesures.lock").open("a") as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        for suite in ["nsh","nsh_scripts","nsh_terminal"]:
            start=time.time_ns()
            with (a.out/(suite+".log")).open("wb") as log:
                result=subprocess.run([sys.executable,"-B",ROOT/f"novaOS/tests/{suite}.py"],env=env,stdout=log,stderr=subprocess.STDOUT,timeout=1200)
            for pattern in [f"cli-{a.arch}-*.serial",f"nsh-{a.arch}-*.json",f"scripts-{a.arch}-*.json"]:
                for path in (ROOT/"novaOS/build").glob(pattern):
                    if path.stat().st_mtime_ns>=start:
                        shutil.copyfile(path,a.out/(suite+"-"+path.name))
            results[suite]=result.returncode
            (a.out/"resultats.json").write_text(json.dumps(dict(arch=a.arch,kernel=sha(kernel),suites=results),indent=1)+"\n")
            print(a.arch,suite,result.returncode,flush=True)
    return int(any(results.values()))
if __name__=="__main__":raise SystemExit(main())
