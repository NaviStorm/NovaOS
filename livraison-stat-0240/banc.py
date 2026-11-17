"""Qualification de stat sur Darwin ARM64."""
import argparse
import itertools
import json
import os
from pathlib import Path
import platform
import resource
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

BASE = Path(__file__).resolve().parent
NOVA = BASE / "bin/stat-darwin-arm64"
GNU = Path(os.environ.get("GNU_STAT","/opt/homebrew/opt/coreutils/libexec/gnubin/stat"))
TMP = Path(os.environ.get("NOVAOS_TMPDIR", BASE.parent.parent / "tmp"))
ENV = dict(os.environ, TMPDIR=str(TMP)+"/", LC_ALL="C", TZ="UTC", QUOTING_STYLE="shell-escape")
ENV.pop("POSIXLY_CORRECT",None)
FIELDS = "a A b B C d D Hd Ld f F g G h i m n Qn N o s r R Hr Lr t T u U w W x X y Y z Z".split()
FS = "a b c d f i l n Qn s S t T".split()

def run(tool,args,**settings):
    with open("texte","rb") as stdin:
        p=subprocess.run(["stat",*args],executable=tool,stdin=stdin,capture_output=True,env={**ENV,**settings})
    return [p.returncode,p.stdout.hex(),p.stderr.hex()]

def options(full):
    Path("texte").write_bytes(b"nova\n")
    Path("rep").mkdir()
    Path("lien").symlink_to("texte")
    Path("brise").symlink_to("absent")
    Path("nom espace").touch()
    Path("-t").touch()
    Path("ferme").mkdir()
    Path("ferme/f").touch()
    Path("ferme").chmod(0)
    cases=[[],["--"],["--","-t"],["-"],["-f","-"],["--help"],["--version"],["-c","--help","texte"],
        ["--help=x"],["-q"],["--erreur"],["--f"],["--format"],["--printf"],["-c"],["--cached"],["--cached=","texte"],
        ["--cached=no","texte"],["-c%n","absent","texte","absent"],["-c%n","ferme/f"],["-Lc%n","brise"],["texte","-c%n"],
        ["--printf=%s","-c%n","texte"],["-c%n","--printf=%s","texte"],["-Lft","-c%n","lien"]]
    for name in ["texte","nom espace","rep","lien","brise","/dev/null"]:
        for flags in [[],["-L"],["--dereference"],["-t"],["--terse"],["-c","%n"],["--format=%N"],["--printf=%n\\0"]]:
            cases.append([*flags,name])
    for opt in ["-f","--file-system","-ft","--fi"]:
        cases.extend([[opt,"-c","%T","texte"],[opt,"absent"]])
    for mode in ["always","never","default","a","n","d"]:
        cases.extend([["--cached="+mode,"texte"],["--cached",mode,"-Ltc%n","lien"]])
    for fs,fields in [(False,FIELDS),(True,FS)]:
        for key in fields:
            for name in (["texte","nom espace","rep","lien"] if full else ["texte"]):
                cases.append([*(["-f"] if fs else []),"-c","%"+key,name])
    for f in [r"\0%n\0",r"\x41\377\t\n",r"\x",r"\e",r"\q","\\","%","%%","%9%","a%n%9","%Qx:%H","%#06a|%10s|%-10.3n|%.9Y",""]:
        cases.append(["--printf="+f,"texte"])
    if full:
        for combo in itertools.product(["-L","--dereference"],["-t","--terse"],["-f","--file-system"],["--format=%n","--printf=%n"]):
            cases.append([*combo,"lien","nom espace"])
    for flags in [["-f"],["--file-system"],["-ft"],["--file-system","--terse"],["--cached=never","-Lf"]]:
        cases.append([*flags,"/dev/null"])
    raw_name=os.fsdecode(b"nom-\xff")
    # APFS refuse ce nom à la création. Comparer le diagnostic de lecture.
    cases.extend([["-c%n",raw_name],["--dere","lien"],["--pr=%n","texte"],["--ter","texte"],
                  ["-c%.N","lien"],["-c%12.2N","lien"],["-c%.Y","texte"],
                  ["-fc%Hd","texte"],["-fc%Qn","nom espace"],["-c","-","--","-t"]])
    for switch in ("-L","-t","-f","--dereference","--terse","--file-system","--cached=default",
                   "-c%s","--format=%s","--printf=%s"):
        cases.extend([[switch],[switch,"--","absent"]])
    rows=[]
    try:
        for args in cases:
            # Lire les liens avant les deux appels pour stabiliser atime.
            if "lien" in args: os.readlink("lien")
            history=[]
            variable=len(args)==4 and args[0]=="-f" and args[2] in ("%a","%c","%d","%f")
            for _ in range(16 if variable else 1):
                a=run(NOVA,args);b=run(GNU,args)
                if a==b:break
                history.append((a,b))

            result="identique" if a==b else "écart"
            if args in (["--help"],["--version"]):
                assert a[0]==0 and not a[2] and a[1]
                result="documentation Nova"
            rows.append(dict(args=args,lectures_precedentes=history,Nova=a,GNU=b,resultat=result))
    finally:
        Path("ferme").chmod(0o700)
    # Le même flux sert aussi d’entrée à un consommateur externe.
    for source,consumer_args in [(["--printf=%n\\0","nom espace","texte"],["-c"]),(["-c%s","-"],["-l"])]:
        results={}
        for label,exe in [("Nova",NOVA),("GNU",GNU)]:
            with open("texte","rb") as inp,subprocess.Popen(["stat",*source],executable=exe,stdin=inp,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=ENV) as producer:
                consumer=subprocess.run(["/usr/bin/wc",*consumer_args],stdin=producer.stdout,capture_output=True)
                producer.stdout.close()
                results[label]=[producer.wait(),producer.stderr.read().hex(),consumer.returncode,consumer.stdout.hex(),consumer.stderr.hex()]
        rows.append(dict(pipe=[source,consumer_args],**results,resultat="identique" if results["Nova"]==results["GNU"] else "écart"))
    for lang,needle in [("fr-CA","Afficher"),("fr-zzzzz","Display"),("en-GB","Display")]:
        assert needle.encode() in bytes.fromhex(run(NOVA,["--help"],LC_ALL=lang)[1])
    v=os.statvfs(".")
    for key,expected in zip(("a","b","c","d","f","s","S"),(v.f_bavail,v.f_blocks,v.f_files,v.f_ffree,v.f_bfree,v.f_frsize,v.f_frsize)):
        for attempt in range(16):
            v=os.statvfs(".")
            expected=dict(a=v.f_bavail,b=v.f_blocks,c=v.f_files,d=v.f_ffree,f=v.f_bfree,s=v.f_frsize,S=v.f_frsize)[key]
            before=int(bytes.fromhex(run(NOVA,["-fc%"+key,"."])[1]))
            after=os.statvfs(".")
            value=dict(a=after.f_bavail,b=after.f_blocks,c=after.f_files,d=after.f_ffree,f=after.f_bfree,s=after.f_frsize,S=after.f_frsize)[key]
            if min(value,expected)<=before<=max(value,expected):break
        rows.append(dict(oracle="statvfs",champ=key,avant=expected,apres=value,Nova=before,resultat="identique" if min(value,expected)<=before<=max(value,expected) else "écart"))
        v=after
    return rows

def measure(tool,paths):
    args=[str(tool),"-c","%i %s %Y","--",*paths]
    start=time.perf_counter_ns()
    with open(os.devnull,"wb") as out:
        p=subprocess.Popen(args,env=ENV,stdout=out)
        _,status,r=os.wait4(p.pid,0)
        p.returncode=os.waitstatus_to_exitcode(status)
        assert p.returncode==0
    return dict(ms=(time.perf_counter_ns()-start)/1e6,rss=r.ru_maxrss)

def performance(full):
    n=2048 if full else 128
    rows=[]
    for load in ["distincts","répété"]:
        for rep in range(6):
            # Les deux exécutables lisent les mêmes inodes. Le premier lecteur alterne.
            order=[("GNU",GNU),("Nova",NOVA)] if rep%2 else [("Nova",NOVA),("GNU",GNU)]
            with tempfile.TemporaryDirectory(dir=".") as dataset:
                names=[str(Path(dataset)/f"f{i:04}") for i in range(n if load=="distincts" else 1)]
                for path in names:Path(path).write_bytes(b"s")
                paths=names if load=="distincts" else names*n
                for i,(label,tool) in enumerate([*order,*order]):
                    phase=("premier" if i==0 else "second" if i==1 else "chaud")
                    rows.append(dict(paire=rep,programme=label,charge=load,phase=phase,n=n,**measure(tool,paths)))
    print("Charge      Passage  Nova ms  GNU ms  Nova objets/s  GNU objets/s  Nova Mio  GNU Mio")
    for load,phase in itertools.product(["distincts","répété"],["premier","chaud"]):
        values=[]
        for label in ["Nova","GNU"]:
            sub=[r for r in rows if (r["charge"],r["phase"],r["programme"])==(load,phase,label)]
            values.append([statistics.median(r[k] for r in sub) for k in ["ms","rss"]])
        (a,ra),(b,rb)=values
        print(f"{load:10} {phase:7} {a:7.2f} {b:7.2f} {1000*n/a:14.0f} {1000*n/b:12.0f} {ra/2**20:9.2f} {rb/2**20:8.2f}")
    return rows

def main():
    action=sys.argv[1]
    program={"options":"tester-options-stat.sh","performance":"mesurer-performances-stat.sh"}[action]
    p=argparse.ArgumentParser(prog=program,description="Qualification de stat sur macOS et NovaOS.")
    m=p.add_mutually_exclusive_group()
    m.add_argument("--rapide",action="store_true");m.add_argument("--complet",action="store_true")
    p.add_argument("--sortie",type=Path,help="fichier JSON sur macOS, répertoire neuf pour QEMU")
    p.add_argument("--novaos",choices=["aarch64","x86_64"],help="rejouer la recette QEMU et ses mesures sur P9/512 et P9/4096")
    a=p.parse_args(sys.argv[2:])
    if a.novaos:
        if not a.sortie:p.error("--novaos demande --sortie avec un répertoire neuf")
        args=[sys.executable,"-B",BASE/"invite.py",a.novaos,a.sortie]
        if not a.complet:args.append("--rapide")
        return subprocess.call(args,env={**os.environ,"NOVAOS_TMPDIR":str(TMP),"TMPDIR":str(TMP)+"/"})
    dest=a.sortie.resolve() if a.sortie else None
    with tempfile.TemporaryDirectory(dir=TMP,prefix="stat-arc027-") as wd:
        original=Path.cwd()
        os.chdir(wd)
        try:rows=(options if action=="options" else performance)(a.complet)
        finally:os.chdir(original)
    report=dict(systeme=platform.platform(),machine=platform.machine(),oracle=str(GNU),gnu=subprocess.check_output([GNU,"--version"],text=True).splitlines()[0],locale="C",tz="UTC",resultats=rows)
    if dest:dest.write_text(json.dumps(report,ensure_ascii=True,indent=1)+"\n")
    if action=="options":
        bad=[r for r in rows if r["resultat"]=="écart"]
        print(f"{len(rows)} contrôles, {len(bad)} écarts");print(*[r.get("args",r.get("champ")) for r in bad],sep="\n")
        return bool(bad)
    return 0

if __name__=="__main__":sys.exit(main())
