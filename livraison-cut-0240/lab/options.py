"""Différentiel en octets avec GNU Coreutils 9.12, options, fichiers et diagnostics."""
import argparse
import gzip
import hashlib
import json
import os
import random
import subprocess
import tempfile
from pathlib import Path

BASE=Path(__file__).resolve().parents[1]
GNU=Path("/opt/homebrew/bin/gcut")
NOVA = Path(os.environ.get("CUT_NOVA_BIN", BASE / "bin/cut-darwin-arm64")).resolve()
def component_version():
    if "CUT_NOVA_VERSION" in os.environ:
        return os.environ["CUT_NOVA_VERSION"].encode("ascii")
    return (BASE.parent / "components/cut/VERSION").read_bytes().strip()
FLAGS=["--bytes","--characters","--fields","--delimiter","--no-partial","--only-delimited",
       "--whitespace-delimited","--output-delimiter","--complement","--zero-terminated","--help","--version"]
TAB=b"A\t\tC\tD\tE\tF\tG\tH\tI\t\nalone\n\tX\tY\nsans fin"
UTF="é界 x\u2003y\t 😀\nplain".encode()+b"\n\xff\xc3(\xc0"
BLANK="  \té\t a \u2003C   \nword\n   \n\n z ".encode()
def corpus(large):
    for a in [["-f1,3,8,9"],["--fields=1,3,8,9"],["--fi","3"],["-f","3-"],["-d",":" ,"-f1,3"],["--delimiter=:", "-f2"],["-sf2"],["--only-delimited","-f2"],["--complement","-f2,7"],["-n","-f2"],["-O","", "-f1,3"],["--output-delimiter=界","-f1,3"],["-f3,1,1-2,2-5"],["-f99"],["-f2","--","-fichier"],["-f1","a","-","b","-"],["-f1","absent","a"],["-f1","repertoire","b"],["-f1","a","--complement"],["--","-f1"]]:
        yield a,TAB if ":" not in str(a) else b"1:2:3\nplain\n"
    for a in [["-c2,4"],["--characters=2-4"],["-b2,4"],["--bytes","2-4"],["-b2,4","-n"],["--bytes=2-4","--no-partial"],["--complement","-nb1,4"],["-O:","-b1-2,3-4"],["-c-3","-O界"],["--char","3"]]:
        yield a,UTF
    for a in [["-w","-f1,3"],["--whitespace-delimited","-f2"],["--whitespace-delimited=trimmed","-sf2"],["--wh=tr","-f1,3"],["--wh=","-f2"],["-F1,3"],["-F","2-"],["-F1-","-O|"],["-F1,2","-d:"],["-w","-F1-","--wh=trimmed"]]:
        yield a,BLANK
    for a in [["-z","-f2"],["--zero-terminated","-c2"],["-f2","-d",""],["-zf1-","-d",""],["-f2","-d","\n"],["-d","界","-f2"],["-d",b"\xa9","-f2"],["-d","ab","-f2"]]:
        yield a,b"a\tb\0\nc\td\0"+"p界q\né".encode()+b"\xa9x"
    for a in [[],["--bad","absent"],["-Q"],["--by"],["--c"],["--o"],["--c=x"],["--version=x"],["-f"],["-b1","-f1"],["-c1","-s"],["-b1","-w"],["-b1","-d,"],["-wf1","-d,"],["-f1","--whitespace-delimited=xyz"],["-f0"],["-b-"],["-c3-1"],["-f18446744073709551615"],["-c18446744073709551616"],["-c12x"],["-b1,,3"]]:
        yield a,TAB
    for a in [["-f1","absent name"],["-f1","absent'é"],["-f1",b"absent\xff"],["--complement","-f1-","-d","\n"],["--complement","-sf1-","-d","\n"]]:
        yield a,b"abc\nx"
    for a in [["-f2"],["-b1"],["-sf1"]]:yield a,b""
    for n in [65535,262143]:
        d=b"x"*n+"界".encode()+b"b\xff\n"
        for a in [["-b"+str(n+2)],["-c"+str(n+1)],["-nb"+str(n+3)],["-f2","-d","界"]]:yield a,d
    # Tranches de lecture entières sans séparateur, y compris au dernier EOF.
    long_field = b"x" * (2 * 65536)
    mixed = b"a\tb\n" + long_field + b"\tfin\n" + long_field
    for args in (["-f1,3"], ["-sf1"], ["-sf2"], ["-f2"]):
        yield args, mixed
    yield ["-z", "-d:", "-sf1"], long_field + b":fin\0" + long_field
    # L’adaptateur peut restituer directement ces blocs complets de 256 Kio.
    block = b"q" * 262144
    for args in (["-f1,3"], ["-sf1"], ["-sf2"], ["-f2"]):
        yield args, block + b"\n" + b"a\tb\n" + block + b"\tfin\n" + block
    yield ["-z", "-d:", "-f1,3"], b"a:b:" + block * 2 + b"\0" + block
    rng=random.Random(27027)
    for _ in range(900 if large else 90):
        data=b"".join(rng.choice([b"x",b" ",b"\t",b"\n",b"\0",b"\xff","é".encode(),"😀".encode(),b":"]) for _ in range(rng.randrange(90)))
        m=rng.choice("bcfF")
        a=["-"+m+",".join(rng.choices(["1","2","-3","2-4","1-","7-","4"],k=rng.randrange(1,5)))]
        if rng.getrandbits(1):a.append("--complement")
        if rng.getrandbits(1):a.append("-z")
        if rng.getrandbits(1):a+=["-O",rng.choice(["","界",":"])]
        if m=="b" and rng.getrandbits(1):a.append("-n")
        if m=="f":a+=rng.choice([[],["-s"],["-w"],["--wh=trimmed"],["-d",":"]])
        yield a,data

def execute(exe,args,src,folder,env):
    with src.open("rb") as f:
        p=subprocess.run(["cut",*args],executable=exe,cwd=folder,stdin=f,capture_output=True,env=env)
        return dict(stdout=p.stdout.hex(),stderr=p.stderr.hex(),rc=p.returncode,stdin_offset=f.tell())

def run(o,folder):
    version_attendue = component_version() if o.novaOS else b"9.12"
    identite = b"cut (NovaOS) " if o.novaOS else b"cut (GNU coreutils) "
    tools = {"GNU": GNU}
    if o.novaOS:
        tools["Nova"] = NOVA
    for name, exe in tools.items():
        version = subprocess.check_output([exe, "--version"], env=dict(os.environ, LC_ALL="C")).splitlines()[0]
        if name == "GNU" and version != b"cut (GNU coreutils) 9.12":
            raise SystemExit(f"Référence GNU 9.12 attendue, obtenu {version!r}")
        print(f"{version.decode()} — {exe} — SHA-256 {hashlib.sha256(exe.read_bytes()).hexdigest()}")
    (folder/"a").write_bytes(b"x\tA");(folder/"b").write_bytes(b"y\tB\n")
    (folder/"-fichier").write_bytes(TAB);(folder/"repertoire").mkdir()
    src=folder/"stdin";failed=0;count=0
    opener=gzip.open if o.jsonl and o.jsonl.suffix==".gz" else open
    with opener(o.jsonl or os.devnull,"wt") as log:
        def record(obj):
            nonlocal failed,count
            count+=1
            if not obj["ok"]:
                print("Écart",obj["args"],obj["locale"],"cas",count)
                failed+=1
            log.write(json.dumps(obj,ensure_ascii=True)+"\n")
        # Une sortie indépendante de GNU rend visible toute perte de TAB ou de LF.
        tsv = b"A\tB\tC\nD\tE\tF\n"
        projected = b"A\tC\nD\tF\n"
        (folder / "donnees.tsv").write_bytes(tsv)
        env = dict(os.environ, LC_ALL="C", LANGUAGE="C")
        for args, source, expected in [
            (["-b1"], "stdin", b"A\nD\n"),
            (["-c1"], "stdin", b"A\nD\n"),
            (["-f1"], "stdin", b"A\nD\n"),
            (["-f1,3", "donnees.tsv"], "fichier", projected),
            (["-f1,3"], "stdin", projected),
        ]:
            row = dict(args=args, locale="C", entree=source, tsv=tsv.hex(),
                       attendu=expected.hex(), mesures={}, ok=True)
            for name, exe in tools.items():
                proc = subprocess.run(["cut", *args], executable=exe, cwd=folder,
                                      env=env, input=tsv if source == "stdin" else b"",
                                      capture_output=True)
                row["mesures"][name] = dict(stdout=proc.stdout.hex(), stderr=proc.stderr.hex(), rc=proc.returncode)
                row["ok"] &= (proc.stdout, proc.stderr, proc.returncode) == (expected, b"", 0)
            record(row)
        for locale in ["C","en_US.UTF-8"]:
            env=dict(os.environ,LC_ALL=locale,LANG="en-US",LANGUAGE="C")
            env.pop("POSIXLY_CORRECT",None)
            for args,data in corpus(o.large):
                src.write_bytes(data)
                row=dict(args=[os.fsdecode(a) for a in args],locale=locale,stdin=data.hex())
                g=execute(GNU,args,src,folder,env);row["GNU"]=g
                row["ok"]=True
                if o.novaOS:
                    n=execute(NOVA,args,src,folder,env);row["Nova"]=n
                    strip=lambda r:bytes.fromhex(r["stderr"]).removeprefix(b"gcut: ").removeprefix(b"cut: ")
                    row["ok"]=n["rc"]==g["rc"] and n["stdout"]==g["stdout"] and strip(n)==strip(g)
                    if g["rc"] and not g["stdout"]:row["ok"] &= n["stdin_offset"]==g["stdin_offset"]
                record(row)
        for tag in ["fr","FR-ca","fr-Latn-FR","en-US","fr--CA","fr_CA.UTF-8"]:
            for opt in ["--help","--version","--ver","--he"]:
                src.write_bytes(b"stdin untouched")
                exe=NOVA if o.novaOS else GNU
                result=execute(exe,[opt],src,folder,dict(os.environ,LC_ALL="C",LANG=tag))
                raw=bytes.fromhex(result["stdout"])
                version=opt.startswith("--v")
                if version:
                    ok = raw.splitlines()[:1] == [identite + version_attendue]
                else:
                    ok = all(v.encode() in raw for v in FLAGS)
                if not version and o.novaOS:ok &= (b"Syntaxe" in raw)==(tag in ["fr","FR-ca","fr-Latn-FR"])
                record(dict(args=[opt],locale=tag,result=result,ok=bool(ok and result["rc"]==0 and not result["stderr"] and result["stdin_offset"]==0)))
        for name,exe in [("GNU",GNU)]+([("Nova",NOVA)] if o.novaOS else []):
            src.write_bytes(TAB*10000)
            rd,wr=os.pipe();os.close(rd)
            try:
                p=subprocess.run([exe,"-f1",src],stdout=wr,stderr=subprocess.PIPE)
            finally:os.close(wr)
            record(dict(args=["-f1","<sortie fermée>"],outil=name,locale="C",rc=p.returncode,stderr=p.stderr.hex(),ok=p.returncode==-13 and not p.stderr))
    print(f"{count-failed}/{count} cas conformes")
    return bool(failed)
def main():
    p=argparse.ArgumentParser(description=__doc__)
    g=p.add_mutually_exclusive_group();g.add_argument("--macOS",action="store_true",help="référence GNU");g.add_argument("--novaOS",action="store_true",help="comparer le Mach-O à GNU (par défaut)")
    p.add_argument("--large",action="store_true",help="900 cas générés par locale")
    p.add_argument("--jsonl",type=Path,help="journal brut, gzip si suffixe .gz")
    o=p.parse_args()
    o.novaOS = not o.macOS
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"],prefix="cut-options-") as d:raise SystemExit(run(o,Path(d)))
if __name__=="__main__":main()
