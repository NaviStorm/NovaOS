"""Comparaison indépendante des processus et des fichiers, dans des répertoires neufs."""
import random
import hashlib
import json
import os
from pathlib import Path
import resource
import select
import signal
import subprocess as sp
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
GNU = Path("/opt/homebrew/bin/gtee")
NOVA = ROOT / "bin/tee"
DATA = random.Random(27).randbytes(20224) + b"\0fin\xff"
MODES = [[], ["-p"], ["--output-error"], *[["--output-error=" + mode] for mode in
         ("warn", "warn-nopipe", "exit", "exit-nopipe")]]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def record(data):
    return {"octets":len(data), "sha256":digest(data)}


def run(exe, args, scenario, lang, posix, data):
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as name:
        cwd = Path(name)
        for f in ("avant", "apres", "existant"):
            (cwd/f).write_bytes(b"initial long\0")
        (cwd/"refus").mkdir()
        env = dict(os.environ, LC_ALL=lang, LANG=lang)
        env.pop("POSIXLY_CORRECT",None)
        if posix:
            env["POSIXLY_CORRECT"]="1"
        kw = dict(cwd=cwd,env=env,stderr=sp.PIPE)
        cmd = [exe,*args]
        if scenario in ("pipe-ferme","pipe-ignore","attente"):
            r,w = os.pipe()
            os.close(r)
            try:
                p=sp.Popen(cmd, stdin=sp.PIPE,stdout=w,restore_signals=scenario!="pipe-ignore",**kw)
                if scenario=="attente":
                    try:
                        p.wait(timeout=3)
                    finally:
                        if p.poll() is None: p.kill()
                _,err=p.communicate(data,timeout=5)
            finally:
                os.close(w)
            out=b""
        elif scenario=="SIGINT":
            p=sp.Popen(cmd,stdin=sp.PIPE,stdout=sp.PIPE,**kw)
            try:
                p.stdin.write(b"pret");p.stdin.flush()
                assert select.select([p.stdout],[],[],5)[0]
                assert os.read(p.stdout.fileno(),4)==b"pret"
                p.send_signal(signal.SIGINT)
                out,err=p.communicate(data,timeout=5)
                out=b"pret"+out
            finally:
                if p.poll() is None: p.kill();p.wait()
        elif scenario=="nonbloquant":
            r,w=os.pipe()
            os.set_blocking(w,False)
            with (cwd/"source").open("w+b") as src:
                src.write(data);src.seek(0)
                p=sp.Popen(cmd,stdin=src,stdout=w,**kw)
            os.close(w)
            time.sleep(.08)
            with os.fdopen(r,"rb") as reader: out=reader.read()
            _,err=p.communicate(timeout=5)
        elif scenario=="limite":
            def limit():
                resource.setrlimit(resource.RLIMIT_FSIZE,(1537,1537))
                signal.signal(signal.SIGXFSZ,signal.SIG_IGN)
            p=sp.run(cmd,input=data,stdout=sp.PIPE,preexec_fn=limit,timeout=5,**kw)
            out,err=p.stdout,p.stderr
        elif scenario=="stdout-lecture":
            with open("/dev/null","rb") as ro:
                p=sp.run(cmd,input=data,stdout=ro,timeout=5,**kw)
            out,err=b"",p.stderr
        elif scenario=="stdin-ferme":
            p=sp.run(cmd,stdout=sp.PIPE,preexec_fn=lambda:os.close(0),timeout=5,**kw)
            out,err=p.stdout,p.stderr
        elif scenario=="stdout-ferme":
            p=sp.run(cmd,input=data,preexec_fn=lambda:os.close(1),timeout=5,**kw)
            out,err=b"",p.stderr
        elif scenario=="lecture-refusee":
            fd=os.open(cwd,os.O_RDONLY)
            try: p=sp.run(cmd,stdin=fd,stdout=sp.PIPE,timeout=5,**kw)
            finally: os.close(fd)
            out,err=p.stdout,p.stderr
        else:
            p=sp.run(cmd,input=data,stdout=sp.PIPE,timeout=5,**kw)
            out,err=p.stdout,p.stderr
        files={os.fsencode(f.name).hex():record(f.read_bytes()) for f in cwd.iterdir() if f.is_file() and f.name!="source"}
        return dict(rc=p.returncode,stdout=record(out),stderr=os.fsdecode(err),files=files),out


def cases():
    for args in ([],["nouveau"],["avant","neuf","apres"],["-"],["--","--append","-"],[""],
                 ["-a","existant"],["existant","--append"],["-aiip","avant","apres"],["--ignore-interrupts","--append","avant"],
                 ["--a","avant"],["--i","--out","avant"],["--output-error","exit","avant"],
                 ["--output-error=warn-n","avant"],["--output-error=exit-","avant"],
                 ["-a","existant","existant"],[f"n{i}" for i in range(13)],
                 ["-a",b"nom-\xff"],["--help","--bad"],["--version","--bad"]):
        yield args,"normal","C",False,DATA
    yield ["existant","-a"],"normal","C",True,DATA
    yield ["existant"],"normal","C",False,b""
    for args in (["-h"],["-v"],["-paZ"],["-p=warn"],["--bad"],["--append=x"],["--i=x"],["--help=x"],
                 ["--output-error=non"],["--output-error="],["--output-error=ex"],["--output-error=w"],["--bad","--help"]):
        yield args,"normal","C",False,DATA
    for longname in ("append","ignore-interrupts","output-error","help","version"):
        for i in range(1,len(longname)+1):
            yield ["--"+longname[:i],"existant"],"normal","C",False,DATA
    for name in ("warn","exit","warn-nopipe","exit-nopipe"):
        for i in range(1,len(name)+1):
            yield ["--out="+name[:i],"avant","refus","apres"],"normal","C",False,DATA
    for locale in ("en_US.UTF-8","fr_FR.UTF-8"):
        for args in (["-aip","existant"],["--output-error=x"],["--bad"]):
            yield args,"normal",locale,False,DATA
    yield ["--=bad"],"normal","C",False,DATA
    for flags in MODES+[["--output-error=exit","-p"],["-p","--output-error=exit"]]:
        for scenario,names in [("normal",["avant","refus","apres"]),("pipe-ferme",["avant","apres"]),("limite",["nouveau","apres"])]:
            yield flags+names,scenario,"C",False,DATA
    for flags in (["-p"],["--output-error=exit-nopipe"]):
        yield flags,"attente","C",False,b""
    yield ["avant"],"pipe-ignore","C",False,DATA
    for flags in ([],["-i"],["--ignore-interrupts"],["-aip"]):
        yield flags+["existant"],"SIGINT","C",False,DATA
    yield ["avant","apres"],"nonbloquant","C",False,DATA*128
    for flags in MODES:
        yield flags+["avant","apres"],"stdout-lecture","C",False,DATA
        yield flags+["absent/a","refus","apres"],"normal","C",False,DATA
    yield ["avant"],"stdin-ferme","C",False,DATA
    yield ["avant"],"stdout-ferme","C",False,DATA
    yield ["avant"],"stdout-ferme","C",False,b""
    yield ["avant"],"lecture-refusee","C",False,DATA
    for lang in ("fr","fr-CA","fr--CA","en-US","de-DE"):
        for option in ("--help","--he","--version","--ver"):
            yield ["existant",option],"normal",lang,False,DATA


def main():
    rows=[]
    for args,scenario,lang,posix,data in cases():
        g,go=run(GNU,args,scenario,lang,posix,data)
        n,no=run(NOVA,args,scenario,lang,posix,data)
        doc=g["rc"]==0 and (go.startswith(b"Usage:") or go.startswith(b"tee (GNU coreutils)"))
        def normalized(s):
            return s.replace(str(GNU),"tee").replace("gtee:","tee:")
        checks=dict(status=g["rc"]==n["rc"],destinations=g["files"]==n["files"],
                    stderr=normalized(g["stderr"])==normalized(n["stderr"]),stdout=go==no)
        if doc:
            checks["stdout"]=((b"nova-cli 0.20.0" in no) if go.startswith(b"tee (GNU coreutils)") else
                              all(k in no for k in (b"--append",b"warn-nopipe",b"exit-nopipe",b"Syntaxe" if lang in ("fr","fr-CA") else b"Usage")))
        rows.append(dict(args=[os.fsdecode(a) for a in args],scenario=scenario,locale=lang,posix=posix,
                         action="documentation" if doc else ("invalide" if "for more information." in g["stderr"] else "copie"),
                         entree=record(data),gnu=g,nova=n,checks=checks))
    report=dict(gnu=str(GNU),gnu_sha256=digest(GNU.read_bytes()),nova_sha256=digest(NOVA.read_bytes()),rows=rows)
    if len(sys.argv)>1: Path(sys.argv[1]).write_text(json.dumps(report,indent=2)+"\n")
    failed=[row for row in rows if not all(row["checks"].values())]
    for row in failed: print(row["scenario"],repr(row["args"]),row["checks"],repr(row["gnu"]["stderr"]),repr(row["nova"]["stderr"]))
    print(f"{len(rows)-len(failed)}/{len(rows)} comparaisons")
    return bool(failed)

if __name__=="__main__":sys.exit(main())
