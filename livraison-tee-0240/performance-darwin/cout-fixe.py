"""Ablation du seul drapeau de liaison sur deux exécutables issus de la même bibliothèque."""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import random
import re
import statistics
import subprocess
import time


def sections(exe):
    desc = subprocess.check_output(["otool","-l",exe],text=True)
    data = exe.read_bytes()
    offset = re.search(r"sectname __text\s+segname __TEXT\s+addr \S+\s+size (\S+)\s+offset (\d+)",desc)
    size, start = int(offset[1],16), int(offset[2])
    return dict(octets=len(data),sha256=hashlib.sha256(data).hexdigest(),texte_octets=size,
                texte_sha256=hashlib.sha256(data[start:start+size]).hexdigest(),
                fixups_octets=int(re.search(r"LC_DYLD_CHAINED_FIXUPS\s+cmdsize \d+\s+dataoff \d+\s+datasize (\d+)",desc)[1]))


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie",type=Path)
    p.add_argument("temoin",type=Path)
    p.add_argument("compact",type=Path)
    p.add_argument("ancien",type=Path)
    p.add_argument("livre",type=Path)
    p.add_argument("bibliotheque",type=Path)
    a=p.parse_args()
    apps={"Témoin":a.temoin,"Compact":a.compact}
    # 32 paires de chaque ordre, mélangées à graine fixe.
    orders=list(itertools.permutations(apps))*32
    random.Random(276).shuffle(orders)
    rows=[]
    expected=subprocess.check_output([a.ancien,"--version"])
    for index,order in enumerate([*itertools.permutations(apps),*orders],start=-2):
        for name in order:
            start=time.monotonic_ns()
            proc=subprocess.Popen([apps[name],"--version"],stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            _,status,usage=os.wait4(proc.pid,0)
            ns=time.monotonic_ns()-start
            proc.returncode=os.waitstatus_to_exitcode(status)
            stdout,stderr=proc.communicate()
            if proc.returncode or stdout!=expected or stderr:raise ValueError(name)
            rows.append(dict(tour=index,outil=name,ns=ns,user_s=usage.ru_utime,system_s=usage.ru_stime))
    records={name:sections(exe) for name,exe in dict(apps,ancien=a.ancien,livre=a.livre).items()}
    medians={name:statistics.median(r["ns"] for r in rows if r["outil"]==name and r["tour"]>=0) for name in apps}
    with a.sortie.open("x") as f:
        json.dump(dict(mach_o=records,bibliotheque=hashlib.sha256(a.bibliotheque.read_bytes()).hexdigest(),
                       medianes_ns=medians,mesures=rows),f,indent=1)
        f.write("\n")
    print(medians)
