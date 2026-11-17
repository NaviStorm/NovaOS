"""Différentiel GNU 9.12. Les enregistrements conservent les octets des deux flux."""
import argparse
import json
import os
from pathlib import Path
import subprocess as sp

HERE = Path(__file__).resolve().parents[1]
NOVA = HERE / "bin/uname"
GNU = os.environ.get("GNU_UNAME", "/opt/homebrew/bin/guname")
FIELDS = "snrvmpio"
NAMES = ["all","all-labeled","kernel-name","sysname","nodename","kernel-release","release",
         "kernel-version","machine","processor","hardware-platform","operating-system","help","version"]
ENV = dict(UNAME_SYSNAME="KERNEL", UNAME_NODENAME="NODE", UNAME_RELEASE="RELEASE",
           UNAME_VERSION="VERSION", UNAME_MACHINE="MACHINE")
VERSION = (HERE.parent/"components/uname/VERSION").read_text().strip()

def invoke(binary, args, env=None, mode="normal"):
    settings = {"PATH":os.environ["PATH"], "LC_ALL":"C", **(env or {})}
    command = ["uname", *args]
    if mode == "stdout-fermé":
        command = ["/bin/sh","-c",'exec 1>&-; exec "$@"',"sh",str(binary),*args]
        p = sp.run(command, env=settings, capture_output=True)
    elif mode == "pipe-fermé":
        read, write = os.pipe()
        os.close(read)
        try:
            p = sp.run(command, executable=binary, env=settings, stdout=write, stderr=sp.PIPE)
        finally:
            os.close(write)
    else:
        p = sp.run(command, executable=binary, env=settings, input=b"entree ignoree\n", capture_output=True)
    return p.returncode, p.stdout or b"", p.stderr

def cases(full):
    yield [], {}, "normal"
    for opt in ["-"+c for c in "aA"+FIELDS] + ["--"+name for name in NAMES]:
        for args in [[opt],["-mr",opt],[opt,"--"],[opt,"x"],[opt+"=x"]] :
            yield args, ENV, "normal"
    for mask in range(1, 256 if full else 17):
        yield ["-"+ "".join(c for i,c in reversed(list(enumerate(FIELDS))) if mask & 1<<i)], ENV, "normal"
    for pref in sorted({name[:i] for name in NAMES for i in range(1,len(name)+1)}):
        yield ["--"+pref], ENV, "normal"
    for args in [["--"],["--","--help"],["--","--"],["-"],["-aA"],["-Aa"],["-appi"],
                 ["-As","-a"],["-a","-A"],["-zz"],["--=x"],["--nope"],[""],
                 ["abc","def"],["abc","--help"],["abc","-x"],["--version","-x"],
                 ["-x","--help"],["--version","--help"],["--help","--version"],["--h=x"],
                 ["a'b"],["a\nb"],["é"],["a\\b"],["a’b"],[b"\xff"],[b"-\xff"]]:
        yield args,ENV,"normal"
    for args in [["x","--version"],["x","--bad"]]:
        yield args,{**ENV,"POSIXLY_CORRECT":"1"},"normal"
    for tag in ["fr","fr-CA","Fr-Latn-419","fr-XXXX","fr--CA","frr","en-US","fr_FR.UTF-8"]:
        for arg in ["--help","--version"]:
            yield [arg],{"LC_ALL":tag},"normal"
    for arg in ["-a","-A","-sn","-r"]:
        yield [arg],{**ENV,"UNAME_SYSNAME":"","UNAME_NODENAME":"deux mots","UNAME_RELEASE":"a\nb"},"normal"
    for arg in ["-a","-A","-snrvm","-p","-i","-o"]:
        yield [arg],{"UNAME_NODENAME":"noeud-recette"},"normal"
    for loc in ["en_US.UTF-8","en_US.ISO8859-1","ja_JP.SJIS"]:
        for arg in [b"\xe9",b"\x83\x5c","a’b","a\u200bb",bytes(range(1,128))]:
            yield [arg],{"LC_ALL":loc},"normal"
    for mode in ["stdout-fermé","pipe-fermé"]:
        for arg in ["-s","-a","--help","--version"]:
            yield [arg],{},mode

def packed(result):
    rc,out,err=result
    return [rc,out.hex(),err.hex()]

def matrix(full,outfile):
    rows=[]
    for args,env,mode in cases(full):
        args=list(map(os.fsencode,args))
        g=invoke(GNU,args,env,mode)
        n=invoke(NOVA,args,env,mode)
        err=g[2].replace(b"guname:",b"uname:",1)
        is_version=g[0]==0 and g[1].startswith(b"uname (GNU coreutils)")
        help_page=g[0]==0 and g[1].startswith(b"Usage: uname ")
        if is_version:
            ok=n==(0,f"uname (nova-uname) {VERSION}\n".encode(),b"")
        elif help_page:
            fr=env.get("LC_ALL") in ("fr","fr-CA","Fr-Latn-419")
            ok=n[0]==0 and not n[2] and (b"Afficher les informations" if fr else b"Print system and kernel") in n[1] and all(("--"+name).encode() in n[1] for name in NAMES)
        else:
            ok=n==(g[0],g[1],err)
        rows.append(dict(argv=[a.hex() for a in args],env=env,mode=mode,gnu=packed(g),nova=packed(n),
                         doc="help" if help_page else "version" if is_version else None,ok=ok))
    outfile.parent.mkdir(parents=True,exist_ok=True)
    outfile.write_text(json.dumps(rows,ensure_ascii=False)+"\n")
    failures=[r for r in rows if not r["ok"]]
    print(len(rows)-len(failures),"/",len(rows),outfile)
    if failures:
        print(failures[:3]);raise SystemExit(1)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode",choices=["rapide","complet"])
    p.add_argument("sortie",type=Path)
    a=p.parse_args();matrix(a.mode=="complet",a.sortie)
