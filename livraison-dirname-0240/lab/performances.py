"""Lancement et traitement, collecte wait4 individuelle sur Darwin."""
import argparse
import hashlib
import json
import os
import platform
import statistics
import subprocess
import time
from pathlib import Path
from options import GNU, NOVA

def measure(binpath, args, dest):
    start=time.perf_counter_ns()
    pid=os.posix_spawn(binpath,["dirname",*args],{"LC_ALL":"C"},
        file_actions=[(os.POSIX_SPAWN_DUP2,dest,1)])
    _,status,usage=os.wait4(pid,0)
    elapsed=time.perf_counter_ns()-start
    assert os.waitstatus_to_exitcode(status)==0
    return dict(ns=elapsed,rss_octets=usage.ru_maxrss,cpu_s=usage.ru_utime+usage.ru_stime)

def main():
    p=argparse.ArgumentParser()
    p.add_argument("profil",choices=["court","complet"])
    p.add_argument("dossier",type=Path)
    a=p.parse_args()
    a.dossier.mkdir(parents=True,exist_ok=True)
    sizes=[(1,8),(32,32),(256,128),(512,512)]
    if a.profil=="complet":
        sizes.extend([(2048,256),(8192,64),(128,4096)])
    rounds=9 if a.profil=="court" else 31
    data=[]
    load=os.getloadavg()
    table=["Taille ou nombre | Traitement | Nova dirname | GNU dirname",
           "-----------------|------------|--------------|------------"]
    for count,length in sizes:
        paths=[("p"+str(i)).ljust(length-2,"x")+"/f" for i in range(count)]
        for nul in (False,True):
            args=(["-z"] if nul else [])+paths
            assert sum(len(arg)+9 for arg in args)+4096 < os.sysconf("SC_ARG_MAX")
            expected=(b"\0" if nul else b"\n").join(path[:-2].encode() for path in paths)+(b"\0" if nul else b"\n")
            observations={name:[] for name in ("Nova","GNU")}
            def validate(exe):
                p=subprocess.run([exe,*args],capture_output=True,env={"LC_ALL":"C"})
                assert (p.returncode,p.stdout,p.stderr)==(0,expected,b"")
            programs=[("Nova",NOVA),("GNU",GNU)]
            for _,exe in programs:
                validate(exe)
            with open(os.devnull,"wb") as dest:
                warm={name:measure(exe,args,dest.fileno()) for name,exe in programs}
                for i in range(rounds):
                    for name,exe in programs[::1 if i%2==0 else -1]:
                        observations[name].append(measure(exe,args,dest.fileno()))
            for _,exe in programs:
                validate(exe)
            med={name:statistics.median(r["ns"] for r in rows) for name,rows in observations.items()}
            table.append(f"{count} × {length} o | {'NUL' if nul else 'LF'} | {med['Nova']/1e6:.3f} ms | {med['GNU']/1e6:.3f} ms")
            data.append(dict(nombre=count,octets_par_chemin=length,nul=nul,oracle_sha256=hashlib.sha256(expected).hexdigest(),
                echauffement=warm,passages=observations,mediane_ns=med,
                chemins_s={k:count*1e9/v for k,v in med.items()},
                pic_rss_octets={k:max(r["rss_octets"] for r in rows) for k,rows in observations.items()}))
    text="\n".join(table)+"\n"
    (a.dossier/"tableau.txt").write_text(text)
    (a.dossier/"mesures.json").write_text(json.dumps(dict(profil=a.profil,plateforme=platform.platform(),charge_avant=load,
        charge_apres=os.getloadavg(),horloge="perf_counter_ns, posix_spawn à wait4",destination=os.devnull,mesures=data),indent=1)+"\n")
    print(text,end="")

if __name__=="__main__":
    main()
