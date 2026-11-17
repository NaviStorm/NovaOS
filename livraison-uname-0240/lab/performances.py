"""Temps mural de processus indépendants, CPU et pic RSS par wait4 sur Darwin."""
import argparse
from contextlib import ExitStack
import fcntl
import json
import os
from pathlib import Path
import platform
from statistics import median
import subprocess
import time
from options import HERE,NOVA,GNU,ENV,invoke

def measure(binary,arg,count,actions,env):
    start=time.perf_counter_ns()
    cpu=0.0
    rss=0
    for _ in range(count):
        pid=os.posix_spawn(str(binary),["uname",arg],env,file_actions=actions)
        _,status,r=os.wait4(pid,0)
        if status: raise RuntimeError((arg,status))
        cpu+=r.ru_utime+r.ru_stime
        rss=max(rss,r.ru_maxrss)
    return dict(ns=time.perf_counter_ns()-start,cpu_s=cpu,rss_octets=rss)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode",choices=["rapide","complet"]);p.add_argument("sortie",type=Path)
    a=p.parse_args();a.sortie.mkdir(parents=True,exist_ok=True)
    env={"LC_ALL":"C","PATH":os.environ["PATH"],**ENV}
    series=[]
    result=dict(systeme=platform.platform(),cpu=subprocess.check_output(["sysctl","-n","machdep.cpu.brand_string"],text=True).strip(),
                charge_initiale=os.getloadavg(),gnu=str(GNU),nova=str(NOVA),mode=a.mode,locale="C",series=series)
    with ExitStack() as stack:
        for name in ["arc027-mesures.lock","cut-performances.lock"]:
            lock=stack.enter_context((Path(os.environ["NOVAOS_TMPDIR"])/name).open("a"))
            fcntl.flock(lock,fcntl.LOCK_EX)
        dest=stack.enter_context(open(os.devnull,"wb"))
        actions=[(os.POSIX_SPAWN_DUP2,dest.fileno(),fd) for fd in (1,2)]
        print("| Sélection | Appels | Nova ms | GNU ms | RSS Nova Kio | RSS GNU Kio |")
        print("|---|---:|---:|---:|---:|---:|")
        for arg in ["-s","-a","-omrs"]:
            expected=invoke(GNU,[arg],ENV)
            assert expected[0]==0 and not expected[2] and expected==invoke(NOVA,[arg],ENV)
            for count in [1,32 if a.mode=="complet" else 8]:
                records={"nova":[],"gnu":[]}
                pair=[("nova",NOVA),("gnu",GNU)]
                for i in range(10 if a.mode=="complet" else 4):
                    for label,binary in (pair if i%2==0 else reversed(pair)):
                        row=measure(binary,arg,count,actions,env)
                        if i: records[label].append(row)
                ms=[median(r["ns"] for r in records[k])/1e6 for k in records]
                rss=[median(r["rss_octets"] for r in records[k])/1024 for k in records]
                series.append(dict(arg=arg,appels=count,octets_par_appel=len(expected[1]),releves=records))
                print(f"| {arg} | {count} | {ms[0]:.3f} | {ms[1]:.3f} | {rss[0]:.0f} | {rss[1]:.0f} |")
        # Sonde distincte du processus : coût de la requête uname(3) de Darwin.
        tmp=Path(os.environ["TMPDIR"])/"requete-darwin"
        subprocess.run(["cc","-O2",HERE/"lab/requete.c","-o",tmp],check=True)
        result["uname_libc_ns"]=[float(subprocess.check_output([tmp])) for _ in range(5)]
    result["charge_finale"]=os.getloadavg()
    (a.sortie/"mesures.json").write_text(json.dumps(result,indent=2)+"\n")

if __name__=="__main__": main()
