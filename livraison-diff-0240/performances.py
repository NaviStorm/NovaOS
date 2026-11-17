from laboratoire import *
import fcntl
import statistics
import time

def measure(binary,args,work,expected):
    with (work/"stdout").open("wb") as out,(work/"stderr").open("wb") as err:
        begin=time.perf_counter_ns()
        proc=subprocess.Popen(["diff",*args],executable=binary,cwd=work,env=ENV,stdin=subprocess.DEVNULL,stdout=out,stderr=err)
        _,status,usage=os.wait4(proc.pid,0)
        elapsed=time.perf_counter_ns()-begin
        proc.returncode=os.waitstatus_to_exitcode(status)
    assert [proc.returncode,digest(work/"stdout"),digest(work/"stderr")]==expected
    return dict(ns=elapsed,rss=usage.ru_maxrss,cpu_user=usage.ru_utime,cpu_system=usage.ru_stime)
def main():
    args=parse("Mesurer sur Darwin les cinq charges synthétiques. Mémoire RSS en octets par processus.")
    dest=(args.sortie or HERE/"preuves"/("perf-complet" if args.complet else "perf-rapide")).resolve();dest.mkdir(exist_ok=True,parents=True)
    g=oracle();runs=5 if args.complet else 3
    with (TMP/"cut-performances.lock").open("a+") as lock,tempfile.TemporaryDirectory(dir=TMP,prefix="diff-perf-") as td:
        fcntl.flock(lock,fcntl.LOCK_EX)
        status=subprocess.run(["ps","-A","-o","pcpu=,comm="],capture_output=True,text=True)
        (dest/"charge-avant.txt").write_text("\n".join(s for s in status.stdout.splitlines() if s.split() and float(s.split()[0])>=5))
        d=Path(td);rows=[];summary=[]
        n=65536 if args.complet else 4096
        lines=[f"ARC-027 {i:08d} le témoin ligne {i%37:02d}\n".encode() for i in range(n)]
        for charge in ("égaux","début","fin","dispersés","arborescence"):
            a=d/"a";b=d/"b"
            if charge=="arborescence":
                a.unlink();b.unlink();a.mkdir();b.mkdir();chunks=64 if args.complet else 8
                for i in range(chunks):
                    for side in (a,b):
                        f=side/f"part-{i:03d}";f.write_bytes(b"".join(lines[i*n//chunks:(i+1)*n//chunks])+(b"fin\n" if i%8==0 and side==b else b""))
                opts=["-r","a","b"]
            else:
                a.write_bytes(b"".join(lines));other=lines.copy()
                indices={"égaux":[],"début":[0],"fin":[n-1],"dispersés":range(1,n,127)}[charge]
                for i in indices:other[i]=b"MOD "+other[i]
                b.write_bytes(b"".join(other));opts=["a","b"]
            size=sum(f.stat().st_size for f in (a,b) if f.is_file()) if charge!="arborescence" else sum(f.stat().st_size for f in d.glob("[ab]/*"))
            ref=execute(g,opts,d)
            if execute(NOVA,opts,d)!=ref:raise ValueError("écart avant chronométrage "+charge)
            expected=[ref[0],*(hashlib.sha256(bytes.fromhex(s)).hexdigest() for s in ref[1:])]
            row=dict(charge=charge,octets=size,sortie=len(ref[1])//2,sha256=expected[1],args=opts,Nova=[],GNU=[])
            for i in range(runs):
                for label,binary in ([("GNU",g),("Nova",NOVA)] if i%2 else [("Nova",NOVA),("GNU",g)]):
                    row[label].append(measure(binary,opts,d,expected))
            rows.append(row)
            summary.append([charge,size,row["sortie"],*((statistics.median(p["ns"] for p in row[k])/1e6,statistics.median(p["rss"] for p in row[k])/2**20,size/(statistics.median(p["ns"] for p in row[k])/1e9)/2**20) for k in ("Nova","GNU"))])
        save(dest/"mesures.json",dict(environnement=environment(),passages=runs,charges=rows))
        table="| Charge | Entrée octets | Diff octets | Nova ms | GNU ms | Nova Mio RSS | GNU Mio RSS | Nova Mio/s | GNU Mio/s |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|\n"
        for name,size,volume,n,g in summary:
            table+=f"| {name} | {size} | {volume} | {n[0]:.2f} | {g[0]:.2f} | {n[1]:.2f} | {g[1]:.2f} | {n[2]:.1f} | {g[2]:.1f} |\n"
        (dest/"MESURES.md").write_text(table);print(table)
if __name__=="__main__":main()
