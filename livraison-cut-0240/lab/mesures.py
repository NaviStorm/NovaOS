"""Comparer GNU cut et le Mach-O sur Darwin ARM64. Les gros profils sont explicites."""
import argparse
import fcntl
import hashlib
import itertools
import json
import os
import random
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
TMP = Path(os.environ["NOVAOS_TMPDIR"])
TOOLS = {"Nova": BASE / "bin/cut-darwin-arm64", "GNU": Path("/opt/homebrew/bin/gcut")}
HASHES = {1000000:"140c61a1630c045e7cc2829c2fde6c1a331d79f0ba751795ad8829d85ca08b5d",
          10000000:"397fb342eb9640a159fd807962b28c47aa10424597c49412e8bbb0ea36c5a508",
          65000000:"d8745d5dbf60ba162748fa6c1c35cd81a38df13191c1a2bc9b93ca440e9a257a"}
SEED = 27027

def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()

def blocks(n, records):
    order = [0]*64 + list(range(2,256))*3
    random.Random(SEED).shuffle(order)
    count, tail = divmod(n-1,len(order))
    period = b"".join(records[i] for i in order)
    while count:
        k = min(count,128)
        yield period*k
        count -= k
    yield b"".join(records[i] for i in order[:tail]) + records[1]

def source_file(folder, n, records, directory):
    paths = ([directory/f"{n}.tsv",directory/str(n)] if directory else
             [TMP/"tests-sort-tsv-59f79ed"/str(n),*sorted(TMP.glob(f"sha256-banc-*/{n}.tsv"))])
    for path in paths:
        if path.is_file() and digest(path)==HASHES.get(n):
            return path
    if directory:
        raise ValueError(f"Corpus de {n} lignes absent ou empreinte incorrecte")
    dest = folder/f"{n}.tsv"
    with dest.open("xb") as f:
        f.writelines(blocks(n,records))
    if n in HASHES and digest(dest)!=HASHES[n]:
        raise ValueError("Corpus régénéré différent de SORT/UNIQ")
    return dest

def measure(exe,args,src,loc):
    env=dict(os.environ,LC_ALL=loc)
    before=time.perf_counter_ns()
    proc=subprocess.Popen([exe,*args,src],stdout=subprocess.DEVNULL,env=env)
    _,status,usage=os.wait4(proc.pid,0)
    elapsed=(time.perf_counter_ns()-before)/1e9
    proc.returncode=os.waitstatus_to_exitcode(status)
    if proc.returncode:
        raise RuntimeError((exe,args,proc.returncode))
    return {"s":elapsed,"rss_octets":usage.ru_maxrss}

def suite(folder,options):
    tools={k:v for k,v in TOOLS.items() if k in options.tools}
    meta=dict(graine=SEED,cache="Cache de fichiers macOS, un échauffement par outil, sans purge",
              destination="/dev/null",plateforme=subprocess.check_output(["uname","-a"],text=True).strip(),
              materiel=subprocess.check_output(["sysctl","hw.model","hw.memsize","hw.logicalcpu"],text=True).strip(),
              libre=shutil.disk_usage(folder).free,binaires={k:dict(path=str(p),sha256=digest(p),
              version=subprocess.check_output([p,"--version"],text=True).splitlines()[0]) for k,p in tools.items()},essais=[])
    def save():
        if options.json:
            options.json.write_text(json.dumps(meta,indent=2,ensure_ascii=False)+"\n")
    def case(label,args,src,outputs,loc="C"):
        expected=hashlib.sha256()
        for part in outputs:expected.update(part)
        row=dict(taille=label,arguments=args,locale=loc,octets=src.stat().st_size,source=str(src),
                 sha256=digest(src),attendu=expected.hexdigest(),outils={})
        for k,exe in tools.items():
            with subprocess.Popen([exe,*args,src],stdout=subprocess.PIPE,env=dict(os.environ,LC_ALL=loc)) as p:
                observed=hashlib.file_digest(p.stdout,"sha256").hexdigest()
                if p.wait() or observed!=row["attendu"]:raise ValueError((k,args,observed,row["attendu"]))
            row["outils"][k]=dict(sortie=observed,echauffement=measure(exe,args,src,loc),passages=[])
        for rep in range(3):
            order=list(tools)
            if rep%2:order.reverse()
            for k in order:row["outils"][k]["passages"].append(measure(tools[k],args,src,loc))
        med={k:statistics.median(p["s"] for p in v["passages"]) for k,v in row["outils"].items()}
        def val(k,rate=False):
            if k not in med:return "—"
            return f"{src.stat().st_size/2**20/med[k]:.2f}" if rate else f"{med[k]:.4f}"
        print(f"{label:>12} | {' '.join(args):<24} | {val('Nova'):>9} | {val('GNU'):>9} | {val('Nova',1):>10} | {val('GNU',1):>9}",flush=True)
        meta["essais"].append(row);save()
    print(f"{'Taille':>12} | {'Traitement':<24} | {'Nova (s)':>9} | {'GNU (s)':>9} | {'Mio/s Nova':>10} | {'Mio/s GNU':>9}",flush=True)
    fields=[list(map(str,(*keys[:6],i,*keys[6:]))) for i,keys in enumerate(itertools.product((2,10),repeat=8))]
    def rows(cols):
        return [("\t".join(row[i] for i in cols)+"\n").encode() for row in fields]
    for n in options.sizes:
        if shutil.disk_usage(folder).free<n*65+2**25:raise OSError("Espace libre insuffisant")
        src=source_file(folder,n,rows(range(9)),options.corpus)
        case(str(n),["-f1,3,8,9"],src,blocks(n,rows((0,2,7,8))))
        case(str(n),["--complement","-f2,7"],src,blocks(n,rows((0,2,3,4,5,7,8))))
        utf=folder/"utf8"
        with utf.open("wb") as f:f.writelines(blocks(n,[("é"+row[0]+"界"+row[1]+"😀\n").encode() for row in fields]))
        expected=[("".join(("é"+row[0]+"界"+row[1]+"😀")[i] for i in (0,2,4))+"\n").encode() for row in fields]
        case(str(n),["-c1,3,5"],utf,blocks(n,expected),"en_US.UTF-8")
    for mib in ([1,16,64] if options.large else [1]):
        late=folder/"tardif";data=b"x"*(mib*2**20)
        late.write_bytes(data+b"\tfin\n")
        for flag,expect in [("-sf1",data+b"\n"),("-sf2",b"fin\n")]:
            case(f"{mib} Mio/ligne",[flag],late,[expect])

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--macOS",action="store_true",help="GNU Homebrew")
    p.add_argument("--novaOS",action="store_true",help="Mach-O CUT")
    p.add_argument("--large",action="store_true",help="1, 10 et 65 millions de lignes")
    p.add_argument("--corpus",type=Path,help="Dossier contenant N.tsv ou N, en lecture seule")
    p.add_argument("--json",type=Path,help="Résultats machine, passages individuels et pics RSS")
    o=p.parse_args()
    if not(o.macOS or o.novaOS):p.error("sélectionner --macOS et/ou --novaOS")
    o.tools=[k for k,enabled in [("Nova",o.novaOS),("GNU",o.macOS)] if enabled]
    o.sizes=list(HASHES) if o.large else [10000]
    TMP.mkdir(parents=True,exist_ok=True)
    with (TMP/"cut-performances.lock").open("a") as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        with tempfile.TemporaryDirectory(dir=TMP,prefix="cut-mesures-") as d:suite(Path(d),o)
if __name__=="__main__":main()
