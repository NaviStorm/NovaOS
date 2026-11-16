"""Banc différentiel local. Les scripts générés sont exécutés avec --sandbox."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import stat
import subprocess
import tempfile

SEED = 270240
FILES = {"one": b"a1\nb2\n", "two": b"c3\n", "prog": b"$p\n", "aux": b"x\ny\n", "dest": b"old\n"}

def cases():
    data = b"a1\nb2\na1\n"
    rows = []
    def add(name, args, stdin=data):
        rows.append((name, args, stdin))
    for name, script in [
        ("no-match", "s/introuvable/X/"), ("address-number", "2p"), ("last", "$p"),
        ("address-regex", "/^b/p"), ("range", "1,2p"), ("negated", "/a/!p"),
        ("zero-range","0,/b/p"), ("step","1~2p"), ("relative","1,+1p"),
        ("BRE-captures", r"s/\([a-z]\)\([0-9]\)/\2\1/p"),
        ("BRE-backref", r"s/\(a\)\1/X/p"), ("ampersand","s@a@[&]@p"),
        ("second-global","s/a/Z/2gp"), ("global","s/a/Z/gp"),
        ("print-first","N;P"), ("delete","2d;p"), ("restart","N;P;D"),
        ("next","n;p"), ("join",r"N;s/\n/ + /;p"), ("hold","h;n;G;p"),
        ("hold-append","H;$ {g;p;}"), ("exchange","h;n;x;p"),
        ("translate","y/ab/AB/;p"), ("label",":again\ns/aa/a/;t again\np"),
        ("branch","1b end\np\n:end"), ("T","s/a/z/;T end\np\n:end"),
        ("append","a more"), ("insert","i first"), ("change","1c changed"),
        ("number","="), ("list","l"), ("read","r aux"), ("read-line","R aux"),
        ("write","w dest"), ("write-first","N;W dest"), ("subst-write","s/a/z/w dest"),
        ("exec","e printf FIXED"), ("subst-exec","s/.*/printf FIXED/ep"),
        ("clear","z;p"), ("filename","F"), ("version-instruction","v 4.10"),
        ("comment","#n\np"), ("invalid","?"), ("invalid-regex","s/[/Z/"),
        ("q","q42"), ("Q","Q37"), ("caseflag","s/A/z/Ip"), ("multiline",r"N;s/^b/B/Mp"),
        ("replacement-case",r"s/a/\U&/p"), ("posix-longest",r"s/\(a\|aa\)/[&]/p"),
    ]:
        sample = b"aaa\n" if name in {"BRE-backref","second-global","global","label","posix-longest"} else data
        add(name, ["-n",script],sample)
    for name,args,stdin in [
        ("witness-ERE",["-nE",r"s/^([a-z])([0-9])$/\2\1/p"],b"a1\nb2\n"),
        ("witness-N",["-n",r"N;s/\n/ + /;p"],b"un\ndeux\n"),
        ("witness-no-match",["s/introuvable/X/"],b"abc\n"), ("witness-q",["q42"],b"une ligne\n"),
        ("witness-files",["-n","-e","1p","-e","$p","--","one"],b""),
        ("mixed-scripts",["--silent","--file=prog","--expression=1p","one","two"],b""),
        ("inter-file",["-n","2,3=","one","two"],b""), ("separate",["-sn","$=","one","two"],b""),
        ("invalid-utf8",["s/a/z/"],b"\xff\0a\n"), ("invalid-regex-dot",["s/./X/g"],b"\xff\xc3\xa9\n"),
        ("null",["--null-data","s/a/z/"],b"a\0x\n\0"), ("null-multi",["-zn","N;P"],b"a\0b\0"),
        ("unterminated",["s/a/z/"],b"a"), ("empty",["p"],b""),
        ("long",["s/a/Z/"],b"a"*131072+b"\n"), ("missing",["p","absent","one"],b""),
        ("unknown",["--oops"],b""), ("posix",["--posix","Q"],data),
        ("unbuffered",["--unbuffered","1q"],data), ("long-ere",["--regexp-extended","s/(a)/X/"],data),
        ("r-alias",["-r","s/(a)/X/"],data), ("binary",["-b","p"],b"\r\n"),
        ("line-length",["--line-length=4","-n","l"],b"abcdefghi\n"),
        ("debug",["--debug","1q"],data), ("sandbox",["--sandbox","w dest\ne printf FIXED"],data),
        ("sandbox-late",["--sandbox","w new\nr aux"],data), ("sandbox-subst",["--sandbox","s/a/b/w dest"],data),
        ("sandbox-exec",["--sandbox","s/a/b/e"],data), ("inplace",["-i","s/a/Z/","one"],b""),
        ("backup",["-i.bak","s/a/Z/","one"],b""), ("star-backup",["--in-place=*.saved","s/a/Z/","one"],b""),
        ("symlink",["-i","s/a/Z/","link"],b""), ("follow",["-i","--follow-symlinks","s/a/Z/","link"],b""),
        ("help",["--help"],b""), ("version",["--version"],b""), ("help-first",["-i","--help"],b""),
        ("long-separate",["--quiet","--separate","$=","one","two"],data),
        ("unicode-dot",["s/./X/g"],"é漢\n".encode()), ("line-no-final-p",["p"],b"a"),
    ]: add(name,args,stdin)
    rng=random.Random(SEED)
    for i in range(100):
        regex="".join(rng.choices(["a","b","[0-9]",".","^","$","(","[",r"\(","*",r"\1"],k=rng.randint(1,7)))
        cmds=rng.choice([["s/"+regex+"/Z/g"],["/"+regex+"/p"],["1{","s/"+regex+"/&/p","}"]])
        add(f"fuzz-{i:03}",["--sandbox","-nE","\n".join(cmds)],bytes(rng.choices(b"ab1\xff\0\n",k=rng.randrange(2,40))))
    return rows

def encoded(data):
    if len(data)<=4096:
        return {"hex":data.hex()}
    return {"bytes":len(data),"sha256":hashlib.sha256(data).hexdigest()}

def snapshot(directory):
    state={}
    for f in sorted(directory.iterdir()):
        st=f.lstat()
        state[f.name]={"mode":stat.S_IMODE(st.st_mode),"uid":st.st_uid,"gid":st.st_gid,
                       "link":os.readlink(f)} if f.is_symlink() else {
                           "mode":stat.S_IMODE(st.st_mode),"uid":st.st_uid,"gid":st.st_gid,"data":encoded(f.read_bytes())}
    return state

def execute(exe,args,data,loc,tmp):
    with tempfile.TemporaryDirectory(dir=tmp) as wd:
        base=Path(wd)
        for name,content in FILES.items(): (base/name).write_bytes(content)
        (base/"one").chmod(0o640)
        (base/"link").symlink_to("one")
        p=subprocess.run([exe,*args],input=data,capture_output=True,cwd=wd,env={**os.environ,"LC_ALL":loc,"LANG":"en-US"},timeout=8)
        return dict(stdout=encoded(p.stdout),stderr=encoded(p.stderr),code=p.returncode,files=snapshot(base))

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("candidate",type=Path);ap.add_argument("out",type=Path);ap.add_argument("--oracle",default="gsed")
    a=ap.parse_args()
    oracle=shutil.which(a.oracle)
    version=subprocess.check_output([oracle,"--version"],env={**os.environ,"LC_ALL":"C"})
    if version.splitlines()[0].split()[-3:]!=[b"(GNU",b"sed)",b"4.10"]:raise SystemExit("GNU sed 4.10 requis")
    tmp=Path(os.environ["NOVAOS_TMPDIR"]).resolve(strict=True)
    a.out.mkdir(exist_ok=True,parents=True)
    (a.out/"gnu-version.txt").write_bytes(os.fsencode(oracle)+b"\n"+version)
    binary=a.candidate.resolve(strict=True)
    for loc in ("C","en_US.UTF-8"):
        result=[]
        for name,args,data in cases():
            ref=execute(oracle,args,data,loc,tmp);got=execute(binary,args,data,loc,tmp)
            diff=[k for k in ref if ref[k]!=got[k]]
            result.append(dict(name=name,args=args,input=encoded(data),reference=ref,observed=got,diff=diff))
        (a.out/(loc+".json")).write_text(json.dumps(dict(locale=loc,seed=SEED,rows=result),ensure_ascii=False)+"\n")
        print(loc,len(result),sum(not r["diff"] for r in result),"identiques")
        print([(r["name"],r["diff"]) for r in result if r["diff"] and not r["name"].startswith("fuzz-")])
        for r in result:
            if r["observed"]["code"]<0: raise RuntimeError(r["name"])
if __name__=="__main__":main()
