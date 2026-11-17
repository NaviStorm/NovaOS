"""Oracle GNU explicite, chemins en octets et comparaison des effets."""
import argparse
import itertools
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile

HERE = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("mode", choices=["rapide","complet"])
parser.add_argument("sortie", type=Path)
args = parser.parse_args()
gnu = shutil.which("gmktemp")
assert gnu and b"GNU coreutils" in subprocess.check_output([gnu,"--version"])
rows = []

def exercise(root, words, pattern=None, kind=None, env=None, doc=None, closed=False):
    settings = dict(os.environ, LC_ALL="C", TMPDIR="env", **{})
    settings.update(env or {})
    cases = []
    for exe in [gnu, HERE/"bin/mktemp"]:
        with tempfile.TemporaryDirectory(dir=root) as test:
            for d in ("env","p","p/sub","refus"):
                (Path(test)/d).mkdir()
            denied = Path(test)/"refus"
            denied.chmod(0o500)
            try:
                # argv[0] commun, comme une installation sous le nom mktemp.
                result = subprocess.run(["mktemp",*words],executable=exe,cwd=test,env=settings,
                    stdout=subprocess.PIPE,stderr=subprocess.PIPE,umask=0o027,
                    preexec_fn=(lambda: os.close(1)) if closed else None)
                out,err,rc=result.stdout,result.stderr,result.returncode
                effect=None
                if pattern and rc==0:
                    assert re.fullmatch(pattern+b"\n",out), (words,out,pattern)
                    obj=os.path.join(os.fsencode(test),out[:-1])
                    if os.path.lexists(obj):
                        info=os.lstat(obj)
                        effect=["dir" if stat.S_ISDIR(info.st_mode) else "file",stat.S_IMODE(info.st_mode),info.st_size if stat.S_ISREG(info.st_mode) else None]
                if closed:
                    assert not list((Path(test)/"env").iterdir())
                cases.append(dict(rc=rc,out=out.hex(),err=err.hex(),effect=effect))
            finally:
                denied.chmod(0o700)
    ref,nova=cases
    ok=ref["rc"]==nova["rc"] and bytes.fromhex(ref["err"]).removeprefix(b"g") == bytes.fromhex(nova["err"]) and ref["effect"]==nova["effect"]
    if doc:
        ok &= nova["rc"]==0 and doc.encode() in bytes.fromhex(nova["out"])
    elif pattern:
        ok &= nova["rc"]==0 and nova["effect"]=={"dir":["dir",0o700,None],"file":["file",0o600,0],None:None}[kind]
    else:
        ok &= ref["out"]==nova["out"] and nova["rc"]==1
    rows.append(dict(argv=[os.fsencode(w).hex() for w in words],env=env or {},closed=closed,doc=doc,pattern=pattern.decode("ascii","backslashreplace") if pattern else None,gnu=ref,nova=nova,ok=ok))

with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as root:
    def case(*a,**kw): exercise(root,*a,**kw)
    x=b"[a-zA-Z0-9]"
    for opts,folder,dry in [([],False,False),(["-d"],True,False),(["--directory"],True,False),(["--dry-run"],False,True),(["-q"],False,False),(["--quiet"],False,False),(["-u"],False,True),(["-t"],False,False),(["-dqu"],True,True)]:
        case(opts,rb"env/tmp\."+x+b"{10}",None if dry else "dir" if folder else "file")
    for pre,path in [(["-p","p"],b"p/"),(["-pp"],b"p/"),(["--tmpdir=p"],b"p/"),(["--tmpdir"],b"env/"),(["--tmpdir="],b"env/"),(["-tp","p"],b"env/"),(["--tmpdir=p///"],b"p/")]:
        case(pre+["XYZ.XXX"],path+rb"XYZ\."+x+b"{3}","file")
    case(["-tp","p","XXX"],b"p/"+x+b"{3}","file",env={"TMPDIR":""})
    case(["--tmpdir=p","sub/zXXX"],b"p/sub/z"+x+b"{3}","file")
    case(["--suffix=X","aXXX"],b"a"+x+b"{3}X","file")
    case(["aXXX","--suff",".ok"],b"a"+x+rb"{3}\.ok","file")
    case(["--suffix=", "XXX"],x+b"{3}","file")
    case(["aXXX-bXXXX.end"],b"aXXX-b"+x+rb"{4}\.end","file")
    case(["--","-XXX"],b"-"+x+b"{3}","file")
    case(["--dry","absent/aXXX"],b"absent/a"+x+b"{3}")
    for words in [["-p"],["--suffix"],["--non"],["-h"],["--d"],["--=x"],["--directory=1"],["--quiet=1"],["--dry-run=1"],["--help=x"],["--version=x"],["--","--help"],["XX"],[""],["-q","a","b"],["--suffix=", "XXX.txt"],["--suffix=/t","XXX"],["-t","dir/XXX"],["--tmpdir=p","/XXX"],["aXXX/b"],["--tmpdir","p","XXX"],["missing/aXXX"],["-d","missing/aXXX"],["-q","missing/aXXX"],["--quiet","refus/fXXX"],["refus/fXXX"],["-u","/dev/null/zXXX"]]:
        case(words)
    case(["XXX","--suffix=z"],env={"POSIXLY_CORRECT":"1"})
    for lang,title in [("fr-CA","Créer un fichier"),("fr-Zzzz","Créer un fichier"),("fr--CA","Create a temporary"),("fr-zzzzz","Create a temporary"),("C","Create a temporary")]:
        for opt in ["--help","--he"]:
            case([opt],doc=title,env={"LC_ALL":lang})
    for opt in ["--version","-V","-dqV"]:
        case([opt],doc="mktemp (nova-mktemp) 0.24.0")
    if args.mode=="complet":
        for flags in itertools.product(["","d"],["","q"],["","u"],["","t"]):
            f="".join(flags)
            case((["-"+f] if f else [])+["--suffix=.txt","pXXX"],(b"env/" if "t" in f else b"")+b"p"+x+rb"{3}\.txt",None if "u" in f else "dir" if "d" in f else "file")
        for opts in [[],["-d"],["-q"],["-dq"],["-u"]]:
            case(opts,closed=True)
        case([b"--suffix=\xff","XXX"])
        for prefix in ["dir","dire","director"]:
            case(["--"+prefix,"XXX"],x+b"{3}","dir")
args.sortie.write_text(json.dumps(rows,indent=1)+"\n")
for row in rows:
    if not row["ok"]: print(row)
print(sum(r["ok"] for r in rows),"/",len(rows))
raise SystemExit(not all(r["ok"] for r in rows))
