"""Temps de bout en bout et maximum résident par processus Darwin."""
import argparse
import fcntl
import hashlib
import json
import os
import platform
import statistics
import subprocess
import tempfile
import time
from pathlib import Path
from options import GNU, NOVA, observe

def measure(exe,args,fd,wanted):
    start=time.perf_counter_ns()
    pid=os.posix_spawn(exe,["realpath",*args],{"LC_ALL":"C"},file_actions=[
        (os.POSIX_SPAWN_DUP2,fd,1),(os.POSIX_SPAWN_DUP2,fd,2)])
    _,status,usage=os.wait4(pid,0)
    elapsed=time.perf_counter_ns()-start
    assert os.waitstatus_to_exitcode(status)==wanted
    return dict(ns=elapsed,rss=usage.ru_maxrss,cpu=usage.ru_stime+usage.ru_utime)

def main():
    p=argparse.ArgumentParser()
    p.add_argument("profil",choices=["rapide","complet"])
    p.add_argument("dest",type=Path)
    a=p.parse_args()
    dest=a.dest.resolve()
    dest.mkdir(parents=True,exist_ok=True)
    rows=[]
    table=["Charge | Chemins | Nova ms | GNU ms | Nova chemins/s | GNU chemins/s | Nova RSS Kio | GNU RSS Kio",
           "---|---:|---:|---:|---:|---:|---:|---:"]
    initial=os.getcwd()
    env={"systeme":platform.platform(),"machine":platform.machine(),"cpu":subprocess.check_output(["sysctl","-n","machdep.cpu.brand_string"],text=True).strip(),"charge_avant":os.getloadavg(),"locale":"C","gnu":str(GNU),"nova":str(NOVA),"sha256":{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in (GNU,NOVA)}}
    with (Path(os.environ["NOVAOS_TMPDIR"])/"arc027-mesures.lock").open("a") as lock, tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as tmp:
        fcntl.flock(lock,fcntl.LOCK_EX)
        os.chdir(tmp)
        try:
            deep="a/b/c/d/e/f/g/h"
            Path(deep).mkdir(parents=True)
            Path("lien").symlink_to(deep)
            count=512 if a.profil=="complet" else 64
            for label,mode,path in [("normalisé","-e","a"),("profond","-e",deep),("lien physique","-P","lien/.."),("lien logique","-L","lien/.."),("lexical","-ms",deep+"/../../i"),("erreurs","-e","absent/x")]:
                args=[mode]+[path]*count
                oracle=observe(GNU,args,{"LC_ALL":"C"})
                assert oracle==observe(NOVA,args,{"LC_ALL":"C"})
                samples={v:[] for v in ("Nova","GNU")}
                programs=[("Nova",NOVA),("GNU",GNU)]
                with open(os.devnull,"wb") as fd:
                    for i in range(1+(9 if a.profil=="complet" else 3)):
                        for name,exe in programs[::1 if i%2 else -1]:
                            row=measure(exe,args,fd.fileno(),oracle["statut"])
                            if i:samples[name].append(row)
                ns=[statistics.median(v["ns"] for v in s) for s in samples.values()]
                rss=[max(v["rss"] for v in s)/1024 for s in samples.values()]
                table.append(f"{label} | {count} | {ns[0]/1e6:.3f} | {ns[1]/1e6:.3f} | {count*1e9/ns[0]:.0f} | {count*1e9/ns[1]:.0f} | {rss[0]:.0f} | {rss[1]:.0f}")
                rows.append(dict(charge=label,option=mode,chemin=path,nombre=count,passages=samples,oracle=oracle))
        finally:
            os.chdir(initial)
    env["charge_apres"]=os.getloadavg()
    (dest/"mesures.json").write_text(json.dumps(dict(environnement=env,lots=rows),indent=1)+"\n")
    (dest/"tableau.md").write_text("\n".join(table)+"\n")
    print("\n".join(table))

if __name__=="__main__":
    main()
