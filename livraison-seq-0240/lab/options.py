"""Comparaison des flux bruts et des statuts sur le Mac de référence."""
import argparse
import base64
import gzip
import json
import os
from pathlib import Path
import select
import signal
import time
import subprocess as sp

HERE = Path(__file__).resolve().parents[1]
GNU = "/opt/homebrew/bin/gseq"
NOVA = str(HERE / "bin/seq")
LOCALES = ("C", "en_US.UTF-8", "fr_FR.UTF-8")


def cases():
    for args in [[], ["0"], ["10"], ["-1"], ["4", "4"], ["9", "2"], ["2", "-1", "4"],
                 ["3", "-1", "-2"], ["-0", "2"], ["1", "200", "402"], ["1", "201", "405"],
                 ["0", ".000001", ".000003"], [".1", "-.1", "-.2"], ["0xF423F", "0xF4240"],
                 ["1", "0x1p-1", "2"], ["+0007", "1e0", "9."], ["+inf", "0"], ["-."],
                 ["1e999"], ["1e-999"], ["2", "0", "bad"], ["--", "-nan"],
                 ["1", "nan", "2"], ["nan(foo)"], ["1", "-0e-20", "5"], [" 5", "5"],
                 ["9 "], ["-"], [""], ["--", "-1", "1"], ["1", "2", "3", "4"],
                 ["2", "--help"], ["2", "-w"], ["--unknown"], ["-wx"], ["--="], ["--f"], ["-ws"],
                 ["--equal=0", "2"], ["--version=1"], ["--help=x"], ["-w", "-f", "%g", "3"],
                 ["-wf%g", "4"], ["-wf", "%d", "1"], ["-f", "invalid", "-f", "%g", "3"]]:
        yield args
    for arg in ["\a", "\b", "\v1", "\f", "a’b", "\u0081", "𝄞", b"--\xff", b"-\xff"]:
        yield [arg]
    yield [b"-f",b"%\xff",b"1"]
    for name,value in (("format","%+.2f"),("separator","<>"),("equal-width",None)):
        for length in range(1,len(name)+1):
            token="--"+name[:length]
            yield [token if value is None else token+"="+value, "8","10"]
    for fmt,n in [("%2147483648f","0"),("%.2147483648f","0"),("%2147483648f","1")]:
        yield ["-f",fmt,n]
    for a in ("-s", "--sep", "--separator"):
        for sep in ("", "\n", "\t", "|", "::", "→", "%", "\\n", "\x01"):
            yield [a, sep, "7", "9"]
    yield [b"-s", b"\xff", b"8", b"10"]
    yield [b"-f", b"\xff%g\x01", b"2"]
    for args in [["-ws,", "8", "10"], ["-s:", "-s", ";", "2"], ["--separator=", "2"],
                 ["--separator=-w", "2"], ["-f", "--help", "1"], ["-s", "--version", "2"],
                 ["--form=%g", "-s", "|", "0", ".5", "1"]]:
        yield args
    for flag in ("-w", "--equal-width", "--e"):
        for ops in (["0008", "10"], ["1", "-1", "-1"], ["-1.e-3", "1"], ["-1.0e-4", "1"],
                    ["001e1","11"], ["+1","2"], ["1","3.0"], ["10","-.1","9.9"],
                    ["1.10000e5","1.10000e5"], ["-.1", ".1", ".11"], ["0999","1e3"],
                    [".89999","1e-7",".8999901"], ["0xF","0x10"]):
            yield [flag, *ops]
    for k in "aAeEfFgG":
        for flags in ("", "-", "+", "#", "0", " ", "'", "0+#"):
            for spec in ("", "12.3", ".0", "."):
                yield ["-f", f"x%%[%{flags}{spec}L{k}]%%", "-1.875", "1.875", "1.875"]
    for fmt in ["", "%", "literal", "%%g", "%g%", "%g%g", "%d", "%n", "%*f", "%.*f",
                "%1$f", "%lf", "%Lg", "%llf", "%.4", "%a %s"]:
        yield ["--format=" + fmt, "1"]
    for fmt, val in [("%.70000f","1"),("%a","0x1p-1074"),("%.0a","0x1.8p+1"),
                     ("%#.3g","9.999"),("%g=","1000000"),("%'.2f","12345.6")]:
        yield ["-f",fmt,val,val]
    for n in (1, 20, 81, 200, 8192):
        yield ["9"*n, "1"+"0"*n]
    yield ["1","0.1","1.2"]
    yield ["0,1","1,1","3,3"]
    yield ["-f","%g","1", "1"]
    yield ["-f%g","1"]


def invoke(exe,args,loc):
    # argv[0] identique pour les diagnostics de getopt et les suggestions d'aide.
    r=sp.run(["seq",*args],executable=exe,capture_output=True,timeout=10,env={**os.environ,"LC_ALL":loc,"LANGUAGE":"C"})
    return r.returncode,r.stdout,r.stderr


def normalize(err):
    return err.removeprefix(b"gseq: ") if err.startswith(b"gseq: ") else err.removeprefix(b"seq: ")


def pack(data):
    return base64.b64encode(data).decode()


def first_lines(proc, count):
    data = bytearray()
    deadline = time.monotonic() + 5
    while data.count(b"\n") < count:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([proc.stdout], [], [], remaining)[0]:
            proc.kill()
            raise TimeoutError("préfixe de la suite infinie absent")
        chunk = os.read(proc.stdout.fileno(), 4096)
        if not chunk:
            break
        data.extend(chunk)
    return b"".join(bytes(data).splitlines(keepends=True)[:count])


def manuals(destination):
    results = []
    for loc, french in [("C", False), ("fr_FR.UTF-8", True), ("fr_CA.UTF-8", True), ("de_DE.UTF-8", False)]:
        env = dict(os.environ, LANG=loc, LC_ALL=loc, MANPATH=str(HERE/"man"), MANPAGER="cat")
        page = HERE/"man"/("fr.UTF-8" if french else "")/"man1/seq.1"
        path = sp.check_output(["/usr/bin/man", "-w", "seq"], env=env, text=True).strip()
        assert Path(path) == page
        with sp.Popen(["/usr/bin/man", "seq"], stdout=sp.PIPE, stderr=sp.PIPE, env=env) as proc:
            raw, err = proc.communicate(timeout=15)
            assert proc.returncode == 0 and not err
        rendered = sp.check_output(["/usr/bin/col", "-b"], input=raw, env=env)
        title = "afficher une suite" if french else "print a sequence"
        assert title.encode() in rendered and b"0.20.0" in rendered
        results.append(dict(locale=loc, page=str(page.relative_to(HERE)), statut=0,
                            stdout=pack(raw), stderr=pack(err)))
    (destination/"man-darwin.json").write_text(json.dumps(results, indent=2)+"\n")


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--out",type=Path,default=Path(os.environ.get("NOVAOS_TMPDIR",HERE.parent.parent/"tmp/seq-arc027"))/"rejeu")
    a=p.parse_args()
    a.out.mkdir(parents=True,exist_ok=True)
    manuals(a.out)
    rows=[]
    errors=[]
    def record(name,passed,**data):
        rows.append(dict(id=len(rows)+1,cas=name,ok=passed,**data))
        if not passed: errors.append(rows[-1])
    for loc in LOCALES:
        for args in cases():
            words=list(map(os.fsencode,args))
            ref=invoke(GNU,words,loc)
            got=invoke(NOVA,words,loc)
            ok=ref[:2]==got[:2] and normalize(ref[2])==normalize(got[2])
            record("différentiel",ok,locale=loc,argv=list(map(pack,words)),
                   gnu=[ref[0],pack(ref[1]),pack(ref[2])],nova=[got[0],pack(got[1]),pack(got[2])])
    for loc,fr in [("en_US.UTF-8",False),("fr_FR.UTF-8",True),("fr--CA",False),("fr-CA",True)]:
        for flag in ["--"+name[:i] for name in ("help","version") for i in range(1,len(name)+1)]:
            code,out,err=invoke(NOVA,["-w",flag,"unused"],loc)
            word=("Afficher" if fr else "Print") if flag.startswith("--h") else "nova-cli 0.20.0"
            record("ARC-026",code==0 and not err and word.encode() in out,locale=loc,option=flag,
                   nova=[code,pack(out),pack(err)])
    for args in (["0"],["1"],["-f","%.70000f","1"]):
        runs=[]
        for exe in (GNU,NOVA):
            r=sp.run(["seq",*args],executable=exe,preexec_fn=lambda:os.close(1),stderr=sp.PIPE,env={**os.environ,"LC_ALL":"C"},timeout=5)
            runs.append([r.returncode,pack(r.stderr)])
        record("descripteur de sortie fermé",runs[0][0]==runs[1][0] and normalize(base64.b64decode(runs[0][1]))==normalize(base64.b64decode(runs[1][1])),
               args=args,gnu=runs[0],nova=runs[1])
    for ignored in (False,True):
        for args,loc in [(["inf"],"C"),(["1",".1","inf"],"C"),(["18446744073709551617","inf"],"C"),
                             (["1","0,5","2"],"fr_FR.UTF-8"),(["0x1P-1","0x1P-2","1"],"C")]:
            runs=[]
            for exe in (GNU,NOVA):
                # Installer SIG_IGN uniquement dans le fils.
                with sp.Popen(["seq",*args],executable=exe,stdout=sp.PIPE,stderr=sp.PIPE,
                              env={**os.environ,"LC_ALL":loc},preexec_fn=(lambda:signal.signal(signal.SIGPIPE,signal.SIG_IGN)) if ignored else None) as proc:
                    first=first_lines(proc,4)
                    proc.stdout.close()
                    code=proc.wait(timeout=5)
                    err=proc.stderr.read()
                    runs.append((code,first,err))
            ref,got=runs
            record("lecteur fermé",got[:2]==ref[:2] and got[0]==(1 if ignored else -signal.SIGPIPE) and normalize(ref[2])==normalize(got[2]),
                   args=args,locale=loc,ignore=ignored,gnu=[ref[0],pack(ref[1]),pack(ref[2])],nova=[got[0],pack(got[1]),pack(got[2])])
    with gzip.open(a.out/"options.jsonl.gz","wt",encoding="utf8") as f:
        for row in rows: print(json.dumps(row,ensure_ascii=False),file=f)
    print(f"{len(rows)-len(errors)}/{len(rows)} contrôles réussis")
    for row in errors[:16]:
        print(row["id"],row["cas"],[base64.b64decode(arg)[:64] for arg in row.get("argv",[])],str(row["nova"])[:160],str(row.get("gnu"))[:160])
    return bool(errors)


if __name__=="__main__":
    raise SystemExit(main())
