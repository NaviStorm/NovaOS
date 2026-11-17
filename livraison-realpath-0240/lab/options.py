"""Comparaison des flux avec le même argv[0] et les mêmes fichiers."""
import argparse
import itertools
import json
import os
import select
import pty
import tty
import subprocess
import tempfile
from pathlib import Path

DELIVERY = Path(__file__).resolve().parents[1]
GNU = Path(os.environ.get("GNU_REALPATH", "/opt/homebrew/opt/coreutils/libexec/gnubin/realpath"))
NOVA = DELIVERY / "bin/realpath"

def observe(exe, args, env):
    proc = subprocess.run(["realpath", *args], executable=exe, capture_output=True,
                          input=b"stdin ignore\n", env=env, timeout=5)
    return dict(statut=proc.returncode, out=proc.stdout.hex(), err=proc.stderr.hex())

def terminal(exe,args,env):
    master,slave=pty.openpty()
    try:
        tty.setraw(slave)
        proc=subprocess.run(["realpath", *args],executable=exe,env=env,stdout=slave,
                            stdin=subprocess.DEVNULL,stderr=subprocess.PIPE,timeout=5)
        marker=b"\0REALPATH_PTY_FIN\0"
        os.write(slave,marker)
        output=b""
        while not output.endswith(marker):
            if not select.select([master],[],[],5)[0]:raise TimeoutError("PTY incomplet")
            output+=os.read(master,4096)
        output=output[:-len(marker)]
        return dict(statut=proc.returncode,out=output.hex(),err=proc.stderr.hex())
    finally:
        if slave>=0:os.close(slave)
        os.close(master)

def fixtures():
    Path("tree/leaf").mkdir(parents=True)
    Path("tree/file").touch()
    Path("alias").symlink_to("tree/leaf")
    Path("absolute").symlink_to(Path("tree/leaf").resolve())
    Path("broken").symlink_to("missing")
    Path("cycle").symlink_to("cycle")
    for i in range(3):Path(f"cycle3-{i}").symlink_to(f"cycle3-{(i+1)%3}")
    Path("secret").mkdir()
    Path("secret/child").touch()
    Path("secret").chmod(0)
    Path("-e").touch()

def cases(full):
    inputs=[".", "/", "//", ".././", "alias/..", "absolute/..", "tree/file", "broken", "cycle", "missing/", "", "missing/a", "secret/child", "tree/file/", "tree/file/.."]
    for option in ["", "-E","--canonicalize","-e","--canonicalize-existing","-m","--canonicalize-missing","-L","--logical","-P","--physical","-s","--strip","--no-symlinks","-q","--quiet","-z","--zero","--relative-base=tree","--relative-to=tree"]:
        yield "option", ([option] if option else [])+inputs
    for args in [[],["--"],["--","-e"],["-x"],["--nope"],["--rel"],["--canon"],["--quiet=bad"],["--relative-to"],["--relative-base"],["-q","--relative-to=","."],["-e","--relative-to=tree/file","."],["-e","--relative-base=tree/file","."],["--help"],["--version"],["--ze","-ms","/a","/b"],["--help","-x"],["tree","-e","missing"],["--relative-to","--help","."],["-","--","--"],["-m","--relative-to=.","cycle3-0","cycle3-0/a"],["-em","--relative-to=A","--relative-to","tree",*inputs]]:
        yield "syntaxe",args
    for mode,links in itertools.product(["-E","-e","-m","-me","-em","-eE"],["-L","-P","-s","-sL","-Ls","-LP","-Ps","-sP"]):
        yield "precedence",[mode,links,"-z","--relative-to=.",*inputs]
    if full:
        for a,b,c in itertools.product(["tree","tree/leaf",".","tree/file","missing",""],["tree","tree/leaf",".","tree2",""],["-m","-e","-E"]):
            yield "relatif",[c,"-q","--relative-to",a,"--relative-base",b,*inputs]
        for links,mode in itertools.product(["-s","-L","-P"],["-m","-E","-e"]):
            yield "octets",[links,mode,b"nom \xff",b"'x",b"z\n",b"tree/file/.",b"alias/../alias/.."]

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("profil", choices=["rapide","complet"])
    parser.add_argument("rapport",type=Path)
    args=parser.parse_args()
    dest=args.rapport.resolve()
    rows=[]
    previous=os.getcwd()
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as d:
        os.chdir(d)
        try:
            fixtures()
            for i,(group,argv) in enumerate(cases(args.profil=="complet")):
                env={"LC_ALL":"C"}
                ref=observe(GNU,argv,env)
                got=observe(NOVA,argv,env)
                doc="--help" in argv[:1] or argv==["--version"]
                ok=ref==got if not doc else got["statut"]==0 and not got["err"] and (b"realpath (nova-realpath)" if argv==["--version"] else b"--canonicalize-existing") in bytes.fromhex(got["out"])
                rows.append(dict(id=i,groupe=group,argv=[os.fsdecode(x) for x in argv],ok=ok,gnu=ref,nova=got))
            for loc in ["C","en_US.UTF-8"]:
                for style in ["literal","shell-escape","c","escape","invalide"]:
                    env={"LC_ALL":loc,"QUOTING_STYLE":style}
                    for argv in [["-m","--relative-to=.","a b","é","a\nb"],["-zm","--relative-to=.","a b"],["-e","é"]]:
                        ref=terminal(GNU,argv,env)
                        got=terminal(NOVA,argv,env)
                        rows.append(dict(id=len(rows),groupe="terminal",argv=argv,env=env,ok=got==ref,gnu=ref,nova=got))
            for args1,env in [([".","-e","missing"],{"POSIXLY_CORRECT":"1","LC_ALL":"C"}),(["-e","é"],{"LC_ALL":"en_US.UTF-8"})]:
                ref=observe(GNU,args1,env); got=observe(NOVA,args1,env)
                rows.append(dict(id=len(rows),groupe="environnement",argv=args1,env=env,ok=got==ref,gnu=ref,nova=got))
        finally:
            Path("secret").chmod(0o700)
            os.chdir(previous)
    dest.write_text(json.dumps(rows,indent=1)+"\n")
    bad=[r["id"] for r in rows if not r["ok"]]
    print(f"{len(rows)-len(bad)}/{len(rows)} cas conformes. Écarts {bad}")
    raise SystemExit(bool(bad))

if __name__=="__main__":
    main()
