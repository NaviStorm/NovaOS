"""Comparaison des processus Darwin. Un passage peut contenir plusieurs lancements."""
import argparse
import datetime
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time
from options import GNU, NOVA, sha

def paths(n):
    return [f"dossier-{i%97}/document-{i:05}.txt/" for i in range(n)]

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("profil",choices=["court","complet"])
    p.add_argument("out",type=Path)
    a=p.parse_args()
    a.out.mkdir(parents=True,exist_ok=True)
    full=a.profil=="complet"
    order=[("Nova",str(NOVA)),("GNU",str(GNU))]
    env={"LC_ALL":"C"}
    limit=os.sysconf("SC_ARG_MAX")
    history=[]
    table=[]
    with open(os.devnull,"wb") as sink:
        actions=[(os.POSIX_SPAWN_DUP2,sink.fileno(),1)]
        def timed(binary,argv,loops):
            peak=0
            start=time.perf_counter_ns()
            for _ in range(loops):
                pid=os.posix_spawn(binary,argv,env,file_actions=actions)
                _,status,usage=os.wait4(pid,0)
                if status: raise RuntimeError(status)
                peak=max(peak,usage.ru_maxrss)
            return (time.perf_counter_ns()-start)/loops,peak
        workloads=[("lancement témoin /usr/bin/true",0),("appel court",1),("lot admis par argv NovaOS",30),("lot Darwin",512)]
        if full:
            workloads.extend([("lot Darwin",2048),("lot Darwin",4096)])
        for label,n in workloads:
            corpus=paths(n)
            argv=["basename","-s.txt",*corpus] if n else ["true"]
            size=sum(len(s)+1+8 for s in argv)+64
            assert size < limit and (n!=30 or 1+sum(2+len(s) for s in argv[1:])<=2560)
            oracle=b"".join(f"document-{i:05}\n".encode() for i in range(n))
            binaries=order if n else [(key,"/usr/bin/true") for key,_ in order]
            for _,binary in binaries:
                assert subprocess.check_output(argv,executable=binary,env=env)==oracle
                timed(binary,argv,3)
            loops=(48 if full else 12) if n<100 else (20 if full else 6)
            measures={key:[] for key,_ in binaries}
            for rep in range(9 if full else 5):
                for key,binary in (binaries if rep%2==0 else reversed(binaries)):
                    ns,peak=timed(binary,argv,loops)
                    measures[key].append(ns)
                    history.append(dict(traitement=label,n=n,passage=rep+1,binaire=key,lancements=loops,
                                        ns_par_appel=ns,noms_par_s=n*1e9/ns,octets_sortie_par_s=len(oracle)*1e9/ns,pic_rss_octets=peak))
            vals=[statistics.median(measures[key])/1e6 for key,_ in binaries]
            table.append(f"{n:17} | {label:32} | {vals[0]:11.3f} ms | {vals[1]:10.3f} ms")
    title="  Taille ou nombre | Traitement                       | Nova basename  | GNU basename"
    output="\n".join([title,*table])+"\n"
    print(output,end="")
    (a.out/"performances.txt").write_text(output)
    processes=subprocess.run(["ps","-Ao","comm="],capture_output=True,text=True)
    active=[Path(line.strip()).name for line in processes.stdout.splitlines()
            if any(term in line.lower() for term in ("qemu-system","rustc","cargo","python","gbasename"))]
    (a.out/"performances.json").write_text(json.dumps(dict(profil=a.profil,date=datetime.datetime.now().astimezone().isoformat(),
        cible=platform.platform(),machine=platform.machine(),arg_max=limit,destination=os.devnull,locale="C",
        GNU=sha(GNU),Nova=sha(NOVA),charge_fin=os.getloadavg(),processus_fin=active,mesures=history),indent=1)+"\n")

if __name__=="__main__": main()
