"""Lancement, création et écriture du nom dans un pipe. RSS Darwin en octets."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import fcntl
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import tempfile
import time

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("mode",choices=["rapide","complet"])
parser.add_argument("sortie",type=Path)
args=parser.parse_args()
root=Path(os.environ["NOVAOS_TMPDIR"])
nova=Path(__file__).resolve().parents[1]/"bin/mktemp"
gnu=shutil.which("gmktemp")
assert gnu and b"GNU coreutils" in subprocess.check_output([gnu,"--version"])
rows=[]
count=32 if args.mode=="rapide" else 256

def run(exe, directory, is_dir, workers, passage):
    with tempfile.TemporaryDirectory(dir=directory) as scratch:
        def create(_):
            with subprocess.Popen([exe,*(["-d"] if is_dir else []),"arc027.XXX"],cwd=scratch,
                                  env=dict(os.environ,LC_ALL="C",TMPDIR=scratch),
                                  stdout=subprocess.PIPE,stderr=subprocess.PIPE) as child:
                _,status,usage=os.wait4(child.pid,0)
                child.returncode=os.waitstatus_to_exitcode(status)
                name=child.stdout.read()
                assert child.returncode==0 and child.stderr.read()==b"" and re.fullmatch(rb"arc027\.[a-zA-Z0-9]{3}\n",name)
                return name[:-1].decode(),usage.ru_maxrss
        with ThreadPoolExecutor(workers) as pool:
            start=time.perf_counter_ns()
            samples=list(pool.map(create,range(count)))
            ns=time.perf_counter_ns()-start
        names={name for name,_ in samples}
        assert len(names)==count and set(os.listdir(scratch))==names
        for name in names:
            p=Path(scratch)/name
            assert (p.is_dir() if is_dir else p.is_file() and p.stat().st_size==0)
            (p.rmdir if is_dir else p.unlink)()
        return dict(exe=str(exe),dossier=is_dir,paralleles=workers,passage=passage,n=count,ns=ns,
                    rss_max=max(m for _,m in samples),noms=sorted(names))

with (root/"arc027-mesures.lock").open("a") as lock:
    fcntl.flock(lock,fcntl.LOCK_EX)
    for is_dir in (False,True):
        for workers in (1,4):
            for i in range(3):
                for exe in ([nova,gnu] if i%2==0 else [gnu,nova]):
                    rows.append(run(exe,os.environ["TMPDIR"],is_dir,workers,i))
args.sortie.write_text(json.dumps(dict(hote=platform.platform(),locale="C",mode=args.mode,mesures=rows),indent=1)+"\n")
print("Type | Processus | Nova ms | GNU ms | Nova/s | GNU/s | RSS max Nova/GNU (Kio)")
for is_dir in (False,True):
    for workers in (1,4):
        times=[]
        rss=[]
        for exe in [nova,gnu]:
            group=[r for r in rows if r["exe"]==str(exe) and r["dossier"]==is_dir and r["paralleles"]==workers]
            times.append(statistics.median(r["ns"] for r in group)/1e6)
            rss.append(max(r["rss_max"] for r in group)/1024)
        print(("Dossier" if is_dir else "Fichier"),workers,*[round(t,1) for t in times],
              *[round(count*1000/t) for t in times],*[round(m) for m in rss],sep=" | ")
