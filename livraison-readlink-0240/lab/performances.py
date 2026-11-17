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
    pid=os.posix_spawn(binpath,["readlink",*args],{"LC_ALL":"C"},
        file_actions=[(os.POSIX_SPAWN_DUP2,dest,1)])
    _,status,usage=os.wait4(pid,0)
    elapsed=time.perf_counter_ns()-start
    assert os.waitstatus_to_exitcode(status)==0
    return dict(ns=elapsed,rss_octets=usage.ru_maxrss,cpu_s=usage.ru_utime+usage.ru_stime)

def main():
    import tempfile
    import fcntl
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profil",choices=["rapide","complet"])
    parser.add_argument("dossier",type=Path)
    a=parser.parse_args()
    out=a.dossier.resolve()
    out.mkdir(parents=True,exist_ok=True)
    count,rounds=(128,3) if a.profil=="rapide" else (4096,9)
    exes=[("Nova",NOVA),("GNU",GNU)]
    results=[]
    table=["| Charge | Chemins | Nova ms | GNU ms | Nova chemins/s | GNU chemins/s | Nova RSS Mio | GNU RSS Mio |",
           "|---|---:|---:|---:|---:|---:|---:|---:|"]
    load=os.getloadavg()
    lock=Path(os.environ["NOVAOS_TMPDIR"])/"arc027-mesures.lock"
    with lock.open("a") as coordination, tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"],prefix="readlink-perf-") as tmp:
        fcntl.flock(coordination,fcntl.LOCK_EX)
        os.chdir(tmp)
        Path("cible").touch()
        for i in range(16):
            os.symlink("cible" if i==0 else f"rang-{i-1}",f"rang-{i}")
        leaves=[f"lien-{i}" for i in range(count)]
        for name in leaves: os.symlink("cible",name)
        for label,args in [("brut",leaves),("chaîne 1",["-e",*leaves]),
                           ("chaîne 4",["-e",*(["rang-3"]*count)]),
                           ("chaîne 16",["-e",*(["rang-15"]*count)]),
                           ("absent",["-m",*["absent/"+s for s in leaves]])]:
            checks=[subprocess.check_output([exe,*args],env={"LC_ALL":"C"}) for _,exe in exes]
            assert checks[0]==checks[1] and len(checks[0].splitlines())==count
            obs={k:[] for k,_ in exes}
            with open(os.devnull,"wb") as sink:
                for i in range(rounds):
                    for k,exe in exes[::1 if i%2==0 else -1]:
                        obs[k].append(measure(exe,args,sink.fileno()))
            ns=[statistics.median(x["ns"] for x in obs[k]) for k,_ in exes]
            rss=[statistics.median(x["rss_octets"] for x in obs[k]) for k,_ in exes]
            table.append(f"| {label} | {count} | {ns[0]/1e6:.2f} | {ns[1]/1e6:.2f} | {count*1e9/ns[0]:.0f} | {count*1e9/ns[1]:.0f} | {rss[0]/2**20:.2f} | {rss[1]/2**20:.2f} |")
            results.append(dict(charge=label,passages=obs,nombre=count,sortie_sha256=hashlib.sha256(checks[0]).hexdigest()))
        os.chdir(out)
    text="\n".join(table)+"\n"
    (out/"tableau.md").write_text(text)
    (out/"mesures.json").write_text(json.dumps(dict(plateforme=platform.platform(),processeur=subprocess.check_output(
        ["sysctl","-n","machdep.cpu.brand_string"],text=True).strip(),charge_avant=load,charge_apres=os.getloadavg(),
        profil=a.profil,locale="C",mesures=results),indent=1)+"\n")
    print(text,end="")
if __name__=="__main__":main()
