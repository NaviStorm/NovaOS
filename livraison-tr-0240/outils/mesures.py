"""Débit sur les octets lus. Un échauffement et trois passages en ordre alterné."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tempfile
import time

ROOT=Path(__file__).resolve().parents[1]
BIN={"Nova":str(ROOT/"bin/tr"),"GNU":"/opt/homebrew/bin/gtr"}
MODES={"Traduction":["a-z","A-Z"],"Suppression":["-d","X"],"Compression":["-s","aX"],"Suppr. + compr.":["-ds","X","a"]}
# Motifs fixes, répétés exactement n fois. Le préfixe du dernier texte allonge la première ligne.
DATA=[("1 M lignes",b"",b"aB0Xz9!\n",1_000_000),
      ("10 M lignes",b"",b"aaaXXXX\n",10_000_000),
      ("65 M lignes",b"a"*(1<<20),b"aaXaXXX\n",65_000_000),
      ("32 Mio binaire",b"",bytes(range(256)),131072)]

def create(path, prefix, pattern, repeats):
    with path.open("xb") as f:
        f.write(prefix)
        block=pattern*8192
        for _ in range(repeats//8192): f.write(block)
        f.write(pattern*(repeats%8192))

def measure(binary,args,path):
    with path.open("rb") as inp, open(os.devnull,"wb") as out:
        start=time.perf_counter_ns()
        p=subprocess.Popen([binary,*args],stdin=inp,stdout=out)
        _,s,r=os.wait4(p.pid,0)
        seconds=(time.perf_counter_ns()-start)*1e-9
        p.returncode=os.waitstatus_to_exitcode(s)
        if p.returncode: raise RuntimeError((binary,args,p.returncode))
    return dict(seconds=seconds,rss_bytes=r.ru_maxrss)

def verify(binary,args,path):
    h=hashlib.sha256()
    size=0
    with path.open("rb") as fd, subprocess.Popen([binary,*args],stdin=fd,stdout=subprocess.PIPE) as p:
        while chunk:=p.stdout.read(1<<20):
            size+=len(chunk)
            h.update(chunk)
        if p.wait(): raise RuntimeError((binary,args))
    return size,h.hexdigest()

def show(rows):
    keys=("Taille","Traitement","Nova (s)","GNU (s)","Mio/s Nova","Mio/s GNU")
    table=[keys]
    for r in rows:
        times={k:statistics.median(v["seconds"] for v in s) for k,s in r["samples"].items()}
        table.append([r["size"],r["mode"],*[f"{times[k]:.4f}" if k in times else "—" for k in BIN],
                      *[f"{r['input']/2**20/times[k]:.1f}" if k in times else "—" for k in BIN]])
    widths=list(map(lambda col:max(map(len,col)),zip(*table)))
    for row in table: print(" | ".join(f"{v:<{w}}" for v,w in zip(row,widths)).rstrip())

def main():
    p=argparse.ArgumentParser(description=__doc__)
    g=p.add_mutually_exclusive_group(required=True)
    for flag,helptext in [("macOS","GNU"),("novaOS","Mach-O Nova"),("comparer","GNU et Nova alternés")]:
        g.add_argument("--"+flag,action="store_true",help=helptext)
    p.add_argument("--large",action="store_true",help="activer 10 M, 65 M et le binaire de 32 Mio")
    p.add_argument("--rapport",type=Path,help="nouveau fichier JSON")
    a=p.parse_args()
    selected=list(BIN) if a.comparer else ["GNU" if a.macOS else "Nova"]
    os.environ["LC_ALL"]="C"
    tmp=Path(os.environ.get("NOVAOS_TMPDIR",str(ROOT.parents[1]/"tmp")))
    os.environ.update(TMPDIR=str(tmp)+"/",NOVAOS_TMPDIR=str(tmp))
    rows=[]
    report=dict(host=platform.platform(),ram=int(subprocess.check_output(["sysctl","-n","hw.memsize"])),
                cache="échauffé, sans purge",destination="/dev/null",locale="C",rows=rows,
                versions={k:subprocess.check_output([v,"--version"]).decode().splitlines()[0] for k,v in BIN.items()},
                sha256={k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in BIN.items()})
    if a.rapport and a.rapport.exists(): p.error("choisir un nouveau rapport")
    with (tmp/"arc027-mesures.lock").open("a") as lock, (tmp/"cut-performances.lock").open("a") as cut, tempfile.TemporaryDirectory(prefix="tr-mesures-",dir=tmp) as work:
        fcntl.flock(lock,fcntl.LOCK_EX)
        fcntl.flock(cut,fcntl.LOCK_EX)
        for label,prefix,pattern,n in (DATA if a.large else DATA[:1]):
            size=len(prefix)+len(pattern)*n
            free=shutil.disk_usage(work).free
            if size>free: raise RuntimeError(f"{free} octets libres pour {size} requis")
            path=Path(work)/"corpus"
            create(path,prefix,pattern,n)
            assert path.stat().st_size==size
            with path.open("rb") as f: digest=hashlib.file_digest(f,"sha256").hexdigest()
            for mode,args in MODES.items():
                expected=verify(BIN["GNU"],args,path)
                entries={k:[] for k in selected}
                row=dict(size=label,mode=mode,args=args,input=size,output=expected[0],sha256_output=expected[1],input_sha256=digest,
                         pattern=pattern.hex(),repeats=n,prefix_a=len(prefix),free=free,samples=entries,order=[])
                for k in selected:
                    assert verify(BIN[k],args,path)==expected
                    measure(BIN[k],args,path)
                for iteration in range(3):
                    for k in (selected if iteration%2==0 else selected[::-1]):
                        entries[k].append(measure(BIN[k],args,path))
                        row["order"].append(k)
                rows.append(row)
            path.unlink()
    show(rows)
    if a.rapport: a.rapport.write_text(json.dumps(report,indent=2,ensure_ascii=False)+"\n")

if __name__=="__main__": main()
