"""Différentiel binaire local. La matrice conserve les arguments, entrées, sorties et statuts."""
import argparse
import json
import os
import random
import subprocess
import tempfile
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
GNU=Path("/opt/homebrew/bin/guniq")
NOVA=ROOT/"bin/uniq-darwin-arm64"
BASE=b"madrid\nMadrid\nLisbon\nRome\nRome\nRome\nMadrid"
OPTIONS=["","-c","--count","-d","--repeated","-D","--all-repeated","--all-repeated=none","--all-repeated=prepend","--all-repeated=separate",
         "--group","--group=separate","--group=prepend","--group=append","--group=both","-u","--unique","-i","--ignore-case",
         "-f 1","--skip-fields=1","-s 1","--skip-chars=1","-w 1","--check-chars=2","-z","--zero-terminated",
         "-cd","-cu","-di","-Di","-s 1 -c","-f 1 -s 1 -w 2","-cD","--group -c","--group -u","--group -D","--group -d","-du","-Dui","-w0","-w999","-1","-Dsep","--group=bad","--all-repeated=bad","--s","--unknown","-X","-w","-sX","-f-1","-w-0","--count=x","--","-","- -","a b c"]
def cases():
    for option in OPTIONS:
        args=option.split()
        yield option or "défaut",args,BASE,{}
    for opt in ["--group=both","--all-repeated=prepend","--all-repeated=separate","-zc","-zw0","-zi"]:
        yield "NUL "+opt,["-z",opt],b"p\nq\0p\nq\0X\0\0z\0z",{}
    for opt in ["-f1","-s1","-w1","-i","-f1 -s1 -w2"]:
        for n,raw in enumerate([b" aa\tXY0\nbb\tXY1\ncc\tXZ1\nsolo\n",
            "éX\nßX\nÉ\né\nâY\n".encode(),b"\xffX\n\xfeX\n\xc3\n\xc2\n",b"A\x00x\nB\x00x\n"]):
            yield f"unités {opt} {n}",opt.split(),raw,{}
    for i,data in enumerate([b"",b"\n",b"\n\n\n",b"one",b"a\nb\na",b"q"*70000+b"\n"+b"q"*70000]):
        yield f"EOF {i}",["-c"],data,{}
    for k in ["199209","200112","200809"]:
        yield "historique "+k,["+1","-c"],b"ax\nbx",{"_POSIX2_VERSION":k}
    for v in [{},{"POSIXLY_CORRECT":"1"}]:
        yield "options après opérande",["-","-c"],BASE,v
    for a in ["-D prepend","--all-repeated prepend","-1 -2","-f2 -1","-Dsi","--group=pre","--skip-f 1","-icf1","-f +1"]:
        yield "syntaxe "+a,a.split(),BASE,{}
    yield "Madrid Lisbon Madrid",["-u"],b"Madrid\nLisbon\nMadrid\n",{}
    yield "Madrid après tri",["-c"],b"Lisbon\nMadrid\nMadrid\n",{}
    for opt in ["-di","-Di","-s1 -c"]:
        yield "manuel "+opt,opt.split(),b"madrid\nMadrid\nLisbon\n",{}
    for seed in range(100):
        rng=random.Random(seed)
        pool=[rng.randbytes(rng.randrange(15)) for _ in range(6)]
        opt=rng.choice(["-c","-iD","-u","--group=both","--all-repeated=separate","-f1","-s2","-w3"])
        delim=rng.choice([b"\0",b"\n"])
        data=delim.join(rng.choice(pool) for _ in range(rng.randrange(35)))
        yield f"seed-{seed}",([opt,"-z"] if delim==b"\0" else [opt]),data,{}

def launch(exe,args,data,lang,env,cwd):
    p=subprocess.run([exe,*args],input=data,capture_output=True,cwd=cwd,env={**os.environ,"LC_ALL":lang,"LANG":"en-US",**env})
    return dict(rc=p.returncode,out=p.stdout.hex(),err=p.stderr.hex())
def norm(b):
    return bytes.fromhex(b).replace(os.fsencode(GNU),b"uniq").replace(os.fsencode(NOVA),b"uniq").replace(b"guniq",b"uniq")
def match(a,b):
    return a["rc"]==b["rc"] and a["out"]==b["out"] and norm(a["err"])==norm(b["err"])
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("target",choices=["GNU","NovaOS"]);p.add_argument("--output",type=Path);p.add_argument("--large",action="store_true")
    args=p.parse_args();exe=GNU if args.target=="GNU" else NOVA;rows=[]
    def add(label,opts,data,loc,env,expected,actual,ok):
        rows.append(dict(case=label,args=opts,input_hex=data.hex(),locale=loc,env=env,expected=expected,actual=actual,pass_test=ok))
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp:
        root=Path(tmp)
        for loc in ["C","en_US.UTF-8"]:
            for label,opts,data,env in cases():
                def capture(binary):
                    out=root/"-c"
                    if out.exists():out.unlink()
                    result=launch(binary,opts,data,loc,env,root)
                    result["named_output"]=out.read_bytes().hex() if out.exists() else None
                    return result
                a=capture(GNU);b=capture(exe)
                add(label,opts,data,loc,env,a,b,match(a,b) and a["named_output"]==b["named_output"])
        for tag in ["fr-CA","FR","fr-FAUX-000","de","en-US"]:
            for flag in ["--help","--version"]:
                b=launch(exe,[flag],b"","C",{"LANG":tag},root);v=bytes.fromhex(b["out"])
                ok=b["rc"]==0 and not b["err"] and (exe==GNU or (b"0.24.0" in v if flag=="--version" else (b"Groupes" if tag in ["FR","fr-CA"] else b"Singletons") in v))
                add(tag,[flag],b"","C",{"LANG":tag},launch(GNU,[flag],b"","C",{},root),b,ok)
        (root/"-cities").write_bytes(BASE)
        for label,opts in [("nom tiret",["--","-cities","dest"]),("absent",["absent"]),("sortie impossible",["-","absent/dest"]),("refus",["interdit"])]:
            denied=root/"interdit";denied.touch();denied.chmod(0)
            def files(binary):
                dest=root/"dest"
                if dest.exists(): dest.unlink()
                r=launch(binary,opts,BASE,"C",{},root)
                r["file"]=dest.read_bytes().hex() if dest.exists() else None
                return r
            a=files(GNU);b=files(exe);add(label,opts,BASE,"C",{},a,b,match(a,b) and a["file"]==b["file"])
        def same(binary):
            f=root/"source";f.write_bytes(BASE);r=launch(binary,["source","source"],b"","C",{},root);r["file"]=f.read_bytes().hex();return r
        a=same(GNU);b=same(exe);add("même fichier (contrat de préservation)",["source","source"],BASE,"C",{},a,b,(b==a if exe==GNU else b["rc"]==1 and b["file"]==BASE.hex()))
        def closed(binary):
            rd,wr=os.pipe();os.close(rd)
            try:
                r=subprocess.run([binary],input=b"x\n",stdout=wr,stderr=subprocess.PIPE)
                return dict(rc=r.returncode,out="",err=r.stderr.hex())
            finally:os.close(wr)
        a=closed(GNU);b=closed(exe);add("lecteur fermé",[],b"x\n","C",{},a,b,match(a,b))
        if args.large:
            n=10_000_019
            data=b"x\n"*n;a=launch(GNU,["-c"],data,"C",{},root);b=launch(exe,["-c"],data,"C",{},root)
            add(f"{n} copies de 780a",["-c"],b"","C",{},a,b,match(a,b) and bytes.fromhex(b["out"])==f"{n} x\n".encode())
    bad=[r for r in rows if not r["pass_test"]]
    if args.output:args.output.write_text(json.dumps(rows,ensure_ascii=False,indent=1)+"\n")
    print(len(rows)-len(bad),"/",len(rows),"conformes")
    for r in bad[:25]:print(r["case"],r["locale"],r["expected"],r["actual"])
    raise SystemExit(bool(bad))
if __name__=="__main__":main()
