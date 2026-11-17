"""Mesure le producteur sur /dev/null. Validation séparée en flux."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess as sp
import time

from options import NOVA, GNU, HERE


def measure(exe, n, sink):
    start=time.monotonic_ns()
    with sp.Popen([exe,str(n)],stdout=sink) as proc:
        _,status,r=os.wait4(proc.pid,0)
        proc.returncode=os.waitstatus_to_exitcode(status)
        assert proc.returncode==0
    duration=(time.monotonic_ns()-start)/1e9
    return {"secondes":duration,"rss_octets":r.ru_maxrss,"valeurs_par_seconde":n/duration}


def digest(exe,n):
    total=0
    sha=hashlib.sha256()
    with sp.Popen([exe,str(n)],stdout=sp.PIPE) as proc:
        while chunk:=proc.stdout.read(1024*1024):
            total+=len(chunk)
            sha.update(chunk)
        assert proc.wait()==0
    return {"octets":total,"sha256":sha.hexdigest()}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--complet",action="store_true",help="1, 10 et 65 millions de valeurs")
    p.add_argument("--out",type=Path,default=Path(os.environ.get("NOVAOS_TMPDIR",HERE.parent.parent/"tmp/seq-arc027"))/"rejeu")
    a=p.parse_args()
    os.environ["LC_ALL"]="C"
    a.out.mkdir(parents=True,exist_ok=True)
    name="complet" if a.complet else "court"
    report=dict(profil=name,plateforme=platform.platform(),destination="/dev/null",echauffements=1,
                passages=5,loadavant=os.getloadavg(),machines={k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in [("Nova",NOVA),("GNU",GNU)]},mesures=[])
    table=["Taille ou nombre | Traitement | Nova seq | GNU seq"]
    with open(os.devnull,"wb") as sink:
        for n in ([1000000,10000000,65000000] if a.complet else [10000,100000,1000000]):
            checks=[digest(exe,n) for exe in [NOVA,GNU]]
            assert checks[0]==checks[1]
            pair=[("Nova",NOVA),("GNU",GNU)]
            row={"valeurs":n,"validation":checks[0],"Nova":[],"GNU":[]}
            for _,exe in pair: measure(exe,n,sink)
            for i in range(5):
                for label,exe in (pair if i%2==0 else reversed(pair)):
                    m=measure(exe,n,sink)
                    m["octets_par_seconde"]=checks[0]["octets"]/m["secondes"]
                    row[label].append(m)
            med=[statistics.median(x["secondes"] for x in row[label]) for label,_ in pair]
            table.append(f"{n:>15,} | génération | {med[0]:.6f} s | {med[1]:.6f} s")
            report["mesures"].append(row)
    report["loadapres"]=os.getloadavg()
    table="\n".join(table)+"\n"
    print(table,end="")
    (a.out/f"mesures-{name}.json").write_text(json.dumps(report,indent=2)+"\n")
    (a.out/f"tableau-{name}.txt").write_text(table)

if __name__=="__main__":
    main()
