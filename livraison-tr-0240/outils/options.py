"""Comparer arguments, stdin, stdout, stderr et statut, sans décodage du flux."""
import argparse
import gzip
import itertools
import json
import locale as host_locale
import os
from pathlib import Path
import random
import subprocess
import sys

DIR = Path(__file__).resolve().parents[1]
GNU = os.environ.get("GNU_TR_BIN", "/opt/homebrew/bin/gtr")
NOVA = os.environ.get("NOVA_TR_BIN", str(DIR / "bin/tr"))
SEED = 270240
ALL = bytes(range(256))
SAMPLE = ALL * 2 + b"aabbaXa   z\x00\x00\xff\xff" + "Été".encode() + b"\xc0\x80\xfe\xe2\x82"

REPRISE_ARGS = ["a-z", "A-Z"]
REPRISE_INPUT = b"abc-123\n"

def cases(large):
    yield REPRISE_ARGS, REPRISE_INPUT, 65536
    flags = ["-c","-C","--complement","-d","--delete","-s","--squeeze-repeats","-t","--truncate-set1","-A"]
    for flag in flags:
        args = [flag, "a-d"]
        if flag not in ("-d","--delete","-s","--squeeze-repeats"):
            args.append("x")
        yield args, SAMPLE, 65536
    for toggles in itertools.product((False, True),repeat=4):
        group = "".join(c for c,on in zip("cdst",toggles) if on)
        for sets in [[],["a"],["a-c","x"],["a","x","z"]]:
            yield (["-"+group] if group else []) + sets,SAMPLE,128
    for name in ["complement","delete","squeeze-repeats","truncate-set1","help","version"]:
        operands = ["a"] if name in ("delete","squeeze-repeats") else ["a","x"]
        for n in range(1,len(name)+1):
            yield ["--"+name[:n],*operands], SAMPLE,128
        yield ["--"+name+"=x"], SAMPLE,1
    for args in [
        ["-A","-d","a","b"],["-A","-d","[:bad:]"],["--"],["--","-a","XY"],["a","-s"],["-d","--","-"],["-da"],["-h"],["-v"],["-x"],["--bad"],["--=x"],
        ["-ds","X","a"],["-cds","a","b"],["-s","a-b","x"],["-s","a","xyz"],["",""],["-t","a",""],["a",""],
        ["-d",""],["-s",""],["a-a","x"],["a-c-e","0-9"],["z-a","x"],[r"\377-\001","a"],
        ["abc","[x*]"],["abc","[x*0]"],["abcdef","a[x*02]c"],["abcdef","[y*]bc[x*]"],
        ["-d","[x*]"],["-ds","a","[x*]"],["abc","[x*+2]"],["abc","[x*09]"],["abc","[x*99999999999999999999999]"],
        ["-d","[=]=]"],["-d","[=x=]"],["x","[=x=]"],["-d","[==]"],["-d","[=xyz=]"],
        ["-d","[::]"],["-d","[:bad:]"],["-d","[foo"],["abc","[]*]"],["abc",r"[x*\9]"],
        ["-d",r"\000-\377"],["-s","\\"],["-d",r"\a\b\f\n\r\t\v\\\007\0\12\377\777\8\-\["],
        ["[:lower:]","[:upper:]"],["[:upper:]","[:lower:]"],["-A","[:lower:]","[:upper:]"],
        ["a[:lower:]","z[:upper:]"],["a","[:upper:]"],["a","x[:upper:]"],["[:lower:]a","[:upper:]"],
        ["a","[:digit:]"],["-c","[:digit:]","xy"],["-ct","[:digit:]","x"],["-c","[:digit:]","[x*]"],
        ["[a*513]b","A[x*]Z"], ["[:*3][:digit:]","a-m"],["[=*2][=c=]","xyz"],
        ["-s","--","-"],["-d","a","x'y"],["--help"],["--version"],["--help","--bad"],["--bad","--help"],
    ]:
        yield args,SAMPLE,7
    for cls in "alnum alpha blank cntrl digit graph lower print punct space upper xdigit".split():
        for opt in ["-cd","-Ad","-ds","-Cs"]:
            args = [opt,f"[:{cls}:]"]
            if opt=="-ds": args.append("a-z")
            yield args,SAMPLE,1
    for size in [1,2,3,7,127,128,129,4096,65536]:
        for data in [b"",b"a"*1000+b"X"+b"a"*1000,b"\x00\xfe\xfe",SAMPLE]:
            for args in [["-ds","X","a"],["-s","ab","a"]]:
                yield args,data,size
    rng=random.Random(SEED)
    tokens=["a-z","[a*2]","[Z*]","[=X=]","[:upper:]","[:lower:]","[:digit:]",r"\303",r"\251",r"\-","a","-","[","X","["]
    for _ in range(1500 if large else 180):
        flag=rng.choice([[],["-s"],["-d"],["-ds"],["-ct"],["-cs"],["-st"]])
        sets=["".join(rng.choices(tokens,k=rng.randrange(0,5))) for _ in range(rng.randint(1,2))]
        yield flag+sets,rng.randbytes(rng.randrange(128))+b"XXaaa",rng.choice([1,13,128,4096])

    for flag in [b"-\xff", b"--d\xe9", b"--=\xff", "-é".encode()]:
        yield [os.fsdecode(flag)], SAMPLE, 7

def run(binary,args,data,locale,size):
    env=dict(os.environ,LC_ALL=locale,LANG="en",LANGUAGE="C",NOVA_TR_READ_SIZE=str(size))
    p=subprocess.run(["tr",*args],executable=binary,input=data,capture_output=True,env=env)
    return dict(status=p.returncode,stdout=p.stdout.hex(),stderr=p.stderr.hex())

def main():
    p=argparse.ArgumentParser(description=__doc__)
    g=p.add_mutually_exclusive_group()
    g.add_argument("--macOS",action="store_true",help="référence GNU seule")
    g.add_argument("--novaOS",action="store_true",help="comparer le Mach-O au GNU (défaut)")
    p.add_argument("--large",action="store_true",help="1500 cas générés par locale")
    p.add_argument("--rapport",type=Path,help="preuves JSONL gzip, entrées et sorties hexadécimales")
    a=p.parse_args()
    compare = not a.macOS
    locales = ("C", "en_US.UTF-8")
    previous = host_locale.setlocale(host_locale.LC_CTYPE)
    for name in locales:
        try:
            host_locale.setlocale(host_locale.LC_CTYPE, name)
        except host_locale.Error:
            p.error(f"locale requise indisponible : {name}")
    host_locale.setlocale(host_locale.LC_CTYPE, previous)
    reference = subprocess.check_output([GNU, "--version"], env=dict(os.environ, LC_ALL="C")).decode().splitlines()[0]
    if "(GNU coreutils)" not in reference:
        p.error(f"référence GNU tr attendue : {reference}")
    print(f"Référence {reference} ({GNU})")
    out=gzip.open(a.rapport,"xt",encoding="utf8") if a.rapport else None
    failed=0
    count=0
    for locale in locales:
        help_output = run(GNU, ["--help"], b"", locale, 65536)["stdout"]
        version_output = run(GNU, ["--version"], b"", locale, 65536)["stdout"]
        for args,data,size in cases(a.large):
            ref=run(GNU,args,data,locale,size)
            got=run(NOVA,args,data,locale,size) if compare else ref
            doc = ref["status"] == 0 and ref["stdout"] in (help_output, version_output)
            if doc and compare:
                output=bytes.fromhex(got["stdout"])
                ok=not got["stderr"] and got["status"]==0 and (b"nova-cli" in output if ref["stdout"] == version_output else b"--squeeze-repeats" in output)
            else:
                # Darwin error() utilise le nom du fichier gtr, getopt utilise argv[0].
                err = bytes.fromhex(ref["stderr"])
                actual = bytes.fromhex(got["stderr"])
                if compare:
                    err = b"\n".join(b"tr: " + line[5:] if line.startswith(b"gtr: ") else line for line in err.split(b"\n"))
                ok = got["status"] == ref["status"] and got["stdout"] == ref["stdout"] and actual == err
            if args == REPRISE_ARGS and data == REPRISE_INPUT:
                expected = dict(status=0, stdout=b"ABC-123\n".hex(), stderr="")
                ok = ok and got == ref == expected
            count+=1
            if out:
                out.write(json.dumps(dict(id=count,reference=reference,locale=locale,args=args,stdin=data.hex(),chunk=size,gnu=ref,observed=got,documentation=doc,ok=ok,seed=SEED))+"\n")
            if not ok:
                failed+=1
                if failed<=12: print(locale,args,"GNU",ref["status"],bytes.fromhex(ref["stderr"]),"Nova",got["status"],bytes.fromhex(got["stderr"]),file=sys.stderr)
    if out: out.close()
    print(f"{count-failed}/{count} cas réussis")
    sys.exit(bool(failed))

if __name__=="__main__": main()
