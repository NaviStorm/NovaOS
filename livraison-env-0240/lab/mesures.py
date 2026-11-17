"""Chronométrage mur, mémoire résidente maximale Darwin, données synthétiques."""
import argparse
import datetime
import fcntl
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time
from options import GNU, NOVA, WITNESS, sha

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode",choices=["rapide","complet"])
    p.add_argument("out",type=Path)
    a=p.parse_args()
    a.out.mkdir(parents=True,exist_ok=True)
    full=a.mode=="complet"
    big=2048 if full else 128
    data={f"ENV_{i:04}":"fictif"*8 for i in range(big)}
    args=[f"ENV_{i:04}=fictif" for i in range(512 if full else 32)]
    tasks=[("afficher quatre entrées",["-0"],dict(list(data.items())[:3])),
           ("afficher grand environnement",["-0"],data),
           ("traiter affectations",["-i0",*args],{}),
           ("lancer enfant témoin",["-i","LANG=fr-CA",str(WITNESS),"0"],{})]
    history=[];table=[]
    with (Path(os.environ["TMPDIR"])/"arc027-mesures.lock").open("a") as lock, open(os.devnull,"wb") as sink:
        fcntl.flock(lock,fcntl.LOCK_EX)
        actions=[(os.POSIX_SPAWN_DUP2,sink.fileno(),1)]
        for task,argv,entries in tasks:
            env={"LC_ALL":"C",**entries}
            programs=[("Nova",NOVA),("GNU",GNU)]
            expected=None
            for _,binary in programs:
                output=subprocess.check_output(["env",*argv],executable=binary,env=env)
                if expected is None: expected=output
                assert output==expected
            by_name={name:[] for name,_ in programs}
            for rep in range(8 if full else 4):
                for name,binary in (programs if rep%2==0 else reversed(programs)):
                    samples=[];rss=[]
                    for _ in range(12 if full else 4):
                        start=time.perf_counter_ns()
                        pid=os.posix_spawn(binary,["env",*argv],env,file_actions=actions)
                        _,status,usage=os.wait4(pid,0)
                        samples.append(time.perf_counter_ns()-start);rss.append(usage.ru_maxrss)
                        if status: raise RuntimeError(status)
                    history.append(dict(charge=task,binaire=name,passage=rep,ns=samples,rss_octets=rss))
                    if rep: by_name[name].append((statistics.mean(samples),statistics.median(rss)))
            row=dict(charge=task,variables=len(env),arguments=len(argv),octets=len(expected))
            for name,samples in by_name.items():
                row[name+"_ms"]=statistics.median(s[0] for s in samples)/1e6
                row[name+"_Kio"]=statistics.median(s[1] for s in samples)/1024
            table.append(row)
    result=dict(date=datetime.datetime.now().astimezone().isoformat(),mode=a.mode,systeme=platform.platform(),machine=platform.machine(),
                cpu=subprocess.check_output(["sysctl","-n","machdep.cpu.brand_string"],text=True).strip(),
                destination="/dev/null",loadavg=os.getloadavg(),GNU=sha(GNU),Nova=sha(NOVA),temoin=sha(WITNESS),table=table,brut=history)
    (a.out/"performances.json").write_text(json.dumps(result,ensure_ascii=False,indent=1)+"\n")
    lines=["| Charge | Variables | Arguments | Sortie (o) | Nova (ms) | GNU (ms) | Nova (Kio) | GNU (Kio) |","|---|---:|---:|---:|---:|---:|---:|---:|"]
    lines += ["| {charge} | {variables} | {arguments} | {octets} | {Nova_ms:.3f} | {GNU_ms:.3f} | {Nova_Kio:.0f} | {GNU_Kio:.0f} |".format(**r) for r in table]
    text="\n".join(lines)+"\n";print(text);(a.out/"performances.md").write_text(text)
if __name__=="__main__":main()
