"""Recette différentielle. Les octets de chaque canal sont conservés en hexadécimal."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import pty
import shutil
import select
import time
import subprocess
import tempfile
import tty

BASE = Path(__file__).resolve().parents[1]
GNU = Path("/opt/homebrew/bin/gbasename")
NOVA = BASE / "bin/basename-darwin-arm64"
STYLES = ["literal", "shell", "shell-always", "shell-escape", "shell-escape-always",
          "c", "c-maybe", "escape", "locale", "clocale", "lit", "sh", "invalid"]
VERSION = (BASE.parent / "components/basename/VERSION").read_bytes().strip()

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def cases():
    yield "simple", [b"un/nom"]
    yield "suffixe-positionnel", [b"d/fichier.rs///", b".rs"]
    yield "suffixe-apres-operande", [b"chemin/t-a", b"-a"]
    for i, suffix in enumerate([b"", b".h", b"abc.h", b"d/abc.h", b"plus-long", b"h/", b"/", b".h.h"]):
        yield f"suffixe-{i}", [b"rep/abc.h///", suffix]
    for option in [b"-a", b"--multiple", b"--m", b"-aa", b"-aaz"]:
        yield option.decode(), [option, b"--", b"", b"/", b"//", b"///", b"//a//", b"foo/./", b"foo/..", b"/.../", b"c:\\d", b"-", b"--help", b"a b", b"foo\n", b"d/\xff"]
    for option in [b"-z", b"--zero", b"--z", b"-zz"]:
        yield option.decode(), [option, b"a/\nname", b"me"]
    for argv in [[b"-s", b".rs"], [b"-s.rs"], [b"--suffix=.rs"], [b"--suffix", b".rs"],
                 [b"--s=.rs"], [b"-as.rs"], [b"-zas.rs"], [b"-s.h", b"-s.rs"], [b"--z",b"-s.rs",b"-a"]]:
        yield "suffix-multiple/" + b"_".join(argv).decode(), [*argv,b"x.rs",b"/b.rs/",b"b.h",b".rs",b"x.rs.rs"]
    # Chaque préfixe long non ambigu est admis par getopt_long.
    for word in [b"multiple",b"suffix",b"zero",b"help",b"version"]:
        for length in range(1,len(word)+1):
            prefix=b"--"+word[:length]
            tail={b"multiple":[b"a/x",b"b/y"],b"suffix":[b".h",b"a/b.h",b"c.h"],
                  b"zero":[b"a/x.h",b".h"],b"help":[],b"version":[]}[word]
            yield "prefixe/"+prefix.decode(), [prefix,*tail]
    for i, argv in enumerate([
        [], [b"-a"], [b"-s"], [b"--su"], [b"-azs"], [b"-s", b"--"], [b"--"], [b"-"],
        [b""], [b"/"], [b"//", b"/"], [b"////", b"//"], [b"--", b"-z"], [b"--", b"--version", b"ion"],
        [b"-a", b"file", b"-z"], [b"x", b"y", b"z"], [b"x",b"y",b"q'u\note"],
        [b"-s", b"-z", b"x-z"], [b"--suffix=", b"a", b"b"], [b"--zero=s"], [b"-h"], [b"-v"],
        [b"-azQ"], [b"--unknown"], [b"--="], [b"--multiple=x"], [b"--help=x"], [b"--version=1"],
        [b"--suffix",b"--help",b"foo--help"], [b"-s",b"",b"f"], [b"\xff/a\xfe",b"\xfe"],
        [b"-a",b"absent/path///",b"no-access/secret"], [b"--help",b"--unknown"],[b"--version",b"-x"],
        [b"--h"],[b"--v"],[b"-z",b"--help"],[b"--unknown",b"--help"],[b"-az",b"--version"],
        [b"-a",*[b"d/n%d"%j for j in range(31)]], [b"a/" + b"b"*2555],
    ]):
        yield f"syntaxe-{i:02d}", argv

def run(path, args, env, *, mode="fichier", cwd=None):
    options = dict(executable=path, args=[b"basename",*args], env=env, cwd=cwd, stderr=subprocess.PIPE)
    if mode == "tty":
        master, slave = pty.openpty()
        try:
            tty.setraw(slave)
            os.set_blocking(master,False)
            out = bytearray()
            with tempfile.TemporaryFile(dir=os.environ["TMPDIR"]) as err:
                with subprocess.Popen(**{**options,"stderr":err},stdout=slave) as child:
                    deadline=time.monotonic()+10
                    try:
                        while True:
                            # Vider aussi pendant l’écriture, même au-delà du tampon PTY.
                            done=child.poll() is not None
                            try:
                                chunk=os.read(master,65536)
                            except BlockingIOError:
                                chunk=b""
                            if chunk:
                                out.extend(chunk)
                            elif done:
                                break
                            else:
                                if time.monotonic()>deadline:
                                    raise TimeoutError("pseudo-terminal")
                                select.select([master],[],[],.005)
                    finally:
                        if child.poll() is None: child.kill()
                err.seek(0)
                return dict(status=child.returncode,stdout=out.hex(),stderr=err.read().hex())
        finally:
            if slave is not None:
                os.close(slave)
            os.close(master)
    elif mode.startswith("epipe"):
        read, write = os.pipe()
        os.close(read)
        try:
            p = subprocess.run(**options,stdout=write,restore_signals=(mode=="epipe"),timeout=10)
        finally:
            os.close(write)
        out = b""
    elif mode == "ebadf":
        with open(os.devnull, "rb") as fd:
            p = subprocess.run(**options,stdout=fd,timeout=10)
        out = b""
    else:
        p = subprocess.run(**options,stdout=subprocess.PIPE,timeout=10)
        out = p.stdout
    return dict(status=p.returncode, stdout=out.hex(), stderr=p.stderr.hex())

def compare(row):
    g,n = row["gnu"],row["nova"]
    if g["status"]!=n["status"]:
        return False
    gout,nout = (bytes.fromhex(p["stdout"]) for p in (g,n))
    gerr,nerr = (bytes.fromhex(p["stderr"]) for p in (g,n))
    if gerr.replace(b"gbasename:",b"basename:") != nerr:
        return False
    if gout.startswith(b"Usage:") or b"GNU coreutils) 9.12" in gout:
        if b"GNU coreutils" in gout.splitlines()[0]:
            return nout == b"basename (NovaOS) "+VERSION+b"\n"
        french = row["locale"] in ("fr","fr-CA","fr-Zzzz")
        return all(flag in nout for flag in [b"--multiple",b"--suffix",b"--zero",b"--version",b"--help"]) and (b"Syntaxe" in nout)==french
    return gout == nout

def campaign(out, upstream):
    out.mkdir(parents=True,exist_ok=True)
    rows=[]
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as d:
        work=Path(d)
        inaccessible=work/"no-access"
        inaccessible.mkdir(mode=0)
        def case(label,argv,locale="C",mode="fichier",style=None):
            env={"LC_ALL":locale} if locale is not None else {}
            if style is not None:
                env["QUOTING_STYLE"]=style
            row=dict(id=len(rows),test=label,locale=locale,mode=mode,style=style,argv=[s.hex() for s in argv])
            for name,path in [("gnu",GNU),("nova",NOVA)]:
                row[name]=run(path,argv,env,mode=mode,cwd=work)
            row["ok"]=compare(row)
            rows.append(row)
        for loc in ["C","en_US.UTF-8"]:
            for label,argv in cases():
                case(label,argv,loc)
            quoted=[b"simple",b"a b",b"a\rb",b"a\nb",b"a\tb",b"'\n",b"x'y",b"x'y$",b"a\\b",b'"',b"",b"d/\xff","été".encode(),"’".encode(),b"x\x7fy"]
            for style in [None,*STYLES]:
                for mode,flag in [("tty",b"-a"),("tty",b"-az"),("fichier",b"-a")]:
                    case("guillemets", [flag,b"--",*quoted],loc,mode,style)
        for loc in ["fr_FR.ISO8859-1","ja_JP.eucJP","fr_FR.UTF-8","C.UTF-8"]:
            for style in STYLES:
                case("encodage-terminal",[b"-a",b"--",b"\xe9",b"\xc3\xa9",b"\xa4\xa2",b"z"],loc,"tty",style)
        for loc in [None,"fr","fr-CA","fr-Zzzz","fr-zzzzz","en-GB","fr--CA"]:
            for opt in [b"--help",b"--version"]:
                case("ARC-026",[opt],loc)
        for mode in ["ebadf","epipe","epipe-ignore"]:
            case("sortie-indisponible",[b"f"],mode=mode)
        inaccessible.chmod(0o700)
        if upstream:
            source=Path(upstream).resolve()
            for name,path in [("gnu",GNU),("nova",NOVA)]:
                dest=work/name
                dest.mkdir()
                # Le banc amont attend un programme nommé « basename ».
                shutil.copy2(path,dest/"basename")
                env=dict(os.environ,PATH=str(dest)+":"+os.environ["PATH"],srcdir=str(source),built_programs="basename",VERBOSE="yes")
                p=subprocess.run(["perl","-I",str(source/"tests"),"-MCuSkip","-MCoreutils",str(source/"tests/misc/basename.pl")],cwd=dest,env=env,capture_output=True)
                (out/f"amont-{name}.log").write_bytes(p.stdout+p.stderr)
                if p.returncode:
                    raise RuntimeError(f"banc Perl {name} : {p.returncode}")
    (out/"options.json").write_text(json.dumps(rows,indent=1)+"\n")
    failed=[r for r in rows if not r["ok"]]
    print(f"{len(rows)-len(failed)}/{len(rows)} comparaisons. SHA-256 GNU {sha(GNU)}, Nova {sha(NOVA)}")
    for r in failed[:12]:
        print(r["id"],r["test"],r["locale"],r["style"],r["gnu"],r["nova"])
    return len(failed)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("out",type=Path)
    p.add_argument("--amont",type=Path)
    a=p.parse_args()
    raise SystemExit(bool(campaign(a.out,a.amont)))
