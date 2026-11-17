"""Mesure de deux processus ARM64 sur le même lot. RSS de chaque fils via wait4."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess as sp
import tempfile
import time
from options import ENV, GNU, NOVA, TMP

def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()

def trial(tool,args,directory,null):
    begin=time.perf_counter_ns()
    with sp.Popen(["date",*args],executable=tool,stdin=sp.DEVNULL,stdout=null,stderr=null,env=ENV,cwd=directory) as child:
        _,status,usage=os.wait4(child.pid,0)
        child.returncode=os.waitstatus_to_exitcode(status)
    return dict(secondes=(time.perf_counter_ns()-begin)/1e9,rss_octets=usage.ru_maxrss,statut=child.returncode)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    modes=p.add_mutually_exclusive_group(required=True)
    modes.add_argument("--rapide",action="store_true");modes.add_argument("--complet",action="store_true")
    p.add_argument("--out",type=Path,default=TMP/"date-arc027/mesures.json")
    a=p.parse_args()
    n=65536 if a.complet else 2048
    repetitions=5 if a.complet else 3
    report=dict(systeme=platform.platform(),machine=platform.machine(),cpu=sp.check_output(["sysctl","-n","machdep.cpu.brand_string"],text=True).strip(),
                TZ="UTC0",LC_ALL="C",repetitions=repetitions,charges=[])
    with (TMP/"arc027-mesures.lock").open("a") as lock,tempfile.TemporaryDirectory(dir=TMP,prefix="date-perf-") as tmp,open(os.devnull,"wb") as null:
        fcntl.flock(lock,fcntl.LOCK_EX)
        report.update(GNU_SHA256=digest(GNU),Nova_SHA256=digest(NOVA))
        directory=Path(tmp)
        print("Charge | dates | Nova ms | GNU ms | Nova dates/s | GNU dates/s | Nova Mio | GNU Mio",flush=True)
        for name,pattern,fmt in [("époques",b"@-1.123456789\n","+%s.%N"),("calendrier",b"2024-02-29 12:34:56\n","+%F %T"),("invalides",b"date-invalide\n","+%F")]:
            src=directory/"dates";src.write_bytes(pattern*n)
            args=["-u","-f","dates",fmt]
            samples={"GNU":[],"Nova":[]};tools={"GNU":GNU,"Nova":NOVA}
            raw={}
            for key,tool in tools.items():
                test=sp.run(["date",*args],executable=tool,cwd=directory,env=ENV,capture_output=True)
                raw[key]=(test.returncode,hashlib.sha256(test.stdout).hexdigest(),hashlib.sha256(test.stderr).hexdigest())
            assert raw["GNU"]==raw["Nova"] and raw["GNU"][0]==int(name=="invalides"),raw
            for i in range(repetitions+1):
                order=["GNU","Nova"] if i%2==0 else ["Nova","GNU"]
                for key in order:
                    observation=trial(tools[key],args,directory,null)
                    assert observation["statut"]==raw[key][0]
                    samples[key].append(observation)
            row=dict(charge=name,dates=n,octets=src.stat().st_size,entree=digest(src),oracle=raw,argv=args,brut=samples)
            for key in tools:
                median=statistics.median(x["secondes"] for x in samples[key][1:])
                row[key]=dict(mediane=median,debit=n/median,rss=statistics.median(x["rss_octets"] for x in samples[key][1:]))
            report["charges"].append(row)
            print(" | ".join([name,str(n)]+[f"{row[k]['mediane']*1000:.2f}" for k in ("Nova","GNU")]+[f"{row[k]['debit']:.0f}" for k in ("Nova","GNU")]+[f"{row[k]['rss']/2**20:.2f}" for k in ("Nova","GNU")]),flush=True)
    a.out.parent.mkdir(exist_ok=True,parents=True);a.out.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n")
if __name__=="__main__":main()
