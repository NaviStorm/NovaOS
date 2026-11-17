"""Mesurer en créneau réservé les cinq charges ARC-027 avec trois exécutables."""
import argparse
import fcntl
import hashlib
import itertools
import json
import os
from pathlib import Path
import random
import statistics
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "livraison-diff-0240"))
from laboratoire import HERE, TMP, ENV, digest, execute, save
from performances import measure


def fixture(d, name, lines):
    n = len(lines)
    if name=="arborescence":
        for side in ["a","b"]:
            path=d/side
            path.mkdir()
            for i in range(64):
                data=b"".join(lines[i*n//64:(i+1)*n//64])
                if side=="b" and i%8==0:data+=b"fin\n"
                (path/f"part-{i:03d}").write_bytes(data)
        return ["-r","a","b"]
    right=lines.copy()
    positions={"égaux":[], "début":[0], "fin":[n-1], "dispersés":range(1,n,127)}
    for i in positions[name]:right[i]=b"MOD "+right[i]
    (d/"a").write_bytes(b"".join(lines))
    (d/"b").write_bytes(b"".join(right))
    return ["a","b"]


def charge():
    ps=subprocess.check_output(["ps","-A","-o","pcpu=,comm="],env=ENV,text=True)
    cpus=0.0
    concurrents=[]
    for line in ps.splitlines():
        fields=line.split(maxsplit=1)
        if len(fields)!=2:continue
        cpu=float(fields[0])
        cpus+=cpu
        proc=Path(fields[1]).name
        if cpu>=5 and (proc.startswith("qemu-system-") or proc in ("rustc","clang","ld","cargo")):
            concurrents.append([proc,cpu])
    return dict(utc=time.time(),cpu_total=round(cpus,2),cpu_logiques=os.cpu_count(),
                loadavg=os.getloadavg(),compilateurs_et_vm_actifs=concurrents)


def stats(raw):
    times=[r["ns"] for r in raw]
    median=statistics.median(times)
    q1,_,q3=statistics.quantiles(times,n=4,method="inclusive")
    return dict(mediane_ns=median,mad_ns=statistics.median(abs(v-median) for v in times),
                q1_ns=q1,q3_ns=q3,min_ns=min(times),max_ns=max(times),
                rss_median=statistics.median(r["rss"] for r in raw))


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--publie",type=Path,default=HERE/"bin/diff")
    p.add_argument("--candidat",required=True,type=Path)
    p.add_argument("--gnu",required=True,type=Path)
    p.add_argument("--serie",required=True,type=int,choices=(1,2))
    p.add_argument("--sortie",required=True,type=Path)
    args=p.parse_args()
    exe={"publie":args.publie.resolve(),"candidat":args.candidat.resolve(),"GNU":args.gnu.resolve()}
    prev=json.loads((HERE/"preuves/perf-complet/mesures.json").read_text())
    hashes={k:digest(v) for k,v in exe.items()}
    assert hashes["publie"]==prev["environnement"]["nova_sha256"]
    assert hashes["GNU"]==prev["environnement"]["gnu_sha256"]
    assert hashes["candidat"]!=hashes["publie"]
    args.sortie.mkdir(parents=True,exist_ok=False)
    rng=random.Random(27110+args.serie)
    names=[r["charge"] for r in prev["charges"]]
    expected={r["charge"]:r for r in prev["charges"]}
    report=dict(serie=args.serie,sha256=hashes,plateforme=subprocess.check_output(["sw_vers"],text=True),charges={})
    with (TMP/"cut-performances.lock").open("a+") as lock,tempfile.TemporaryDirectory(dir=TMP,prefix="diff-decision-") as tmp:
        fcntl.flock(lock,fcntl.LOCK_EX)
        lines=[f"ARC-027 {i:08d} le témoin ligne {i%37:02d}\n".encode() for i in range(65536)]
        for name in rng.sample(names,len(names)):
            d=Path(tmp)/name
            d.mkdir()
            opts=fixture(d,name,lines)
            files=[f for f in d.rglob("*") if f.is_file()]
            size=sum(f.stat().st_size for f in files)
            ref=execute(exe["GNU"],opts,d)
            signature=[ref[0],*(hashlib.sha256(bytes.fromhex(v)).hexdigest() for v in ref[1:])]
            assert size==expected[name]["octets"] and signature[1]==expected[name]["sha256"]
            for binary in exe.values():assert execute(binary,opts,d)==ref
            row=dict(octets=size,sortie=len(ref[1])//2,signature=signature,
                     fichiers={str(f.relative_to(d)):digest(f) for f in sorted(files)},
                     charge_avant=charge(),passages={k:[] for k in exe},ordres=[])
            for label in rng.sample(list(exe),3):measure(exe[label],opts,d,signature)
            for _ in range(3):
                orders=list(itertools.permutations(exe))
                rng.shuffle(orders)
                for order in orders:
                    row["ordres"].append(order)
                    for label in order:row["passages"][label].append(measure(exe[label],opts,d,signature))
            row["charge_apres"]=charge()
            row["statistiques"]={k:stats(v) for k,v in row["passages"].items()}
            candidate=row["statistiques"]["candidat"]["mediane_ns"]
            gnu=row["statistiques"]["GNU"]["mediane_ns"]
            row["ratio"]=candidate/gnu
            row["conforme"]=10*candidate<=11*gnu
            report["charges"][name]=row
            save(args.sortie/"mesures.json",report)
            print(name, row["ratio"], row["conforme"],flush=True)
    report["pire_ratio"]=max(r["ratio"] for r in report["charges"].values())
    save(args.sortie/"mesures.json",report)
    return int(any(not r["conforme"] for r in report["charges"].values()))


if __name__=="__main__":raise SystemExit(main())
