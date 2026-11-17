"""Comparaison octet par octet des deux flux et du statut, même argv[0]."""
import argparse
import json
import os
from pathlib import Path
import pty
import random
import select
from concurrent.futures import ThreadPoolExecutor
import subprocess
import tempfile
import tty

BASE = Path(__file__).resolve().parents[1]
GNU = Path("/opt/homebrew/bin/gdirname")
NOVA = BASE/"bin/dirname"
STYLES = "literal shell shell-always shell-escape shell-escape-always c c-maybe escape locale clocale".split()

def invoke(binary, args, env, device):
    env = {"LC_ALL":"C", **env}
    invocation=env.pop("ARGV0_TEST","dirname")
    kw = dict(executable=binary,env=env,stderr=subprocess.PIPE)
    if device == "pty":
        master,slave = pty.openpty()
        try:
            tty.setraw(slave)
            # Darwin peut signaler la fin du fils avant la lisibilité du PTY.
            # La sentinelle suit ses octets sur le même esclave, en mode brut.
            marker=b"\0DIRNAME_FIN_PTY_27024\0"
            def drain():
                buf=bytearray()
                while not buf.endswith(marker):
                    if not select.select([master],[],[],10)[0]:
                        raise TimeoutError("PTY incomplet")
                    buf.extend(os.read(master,8192))
                return bytes(buf[:-len(marker)])
            with ThreadPoolExecutor(max_workers=1) as reader:
                pending=reader.submit(drain)
                try:
                    p=subprocess.run([invocation,*args],stdout=slave,timeout=10,**kw)
                finally:
                    os.write(slave,marker)
                out=pending.result()
        finally:
            os.close(master)
            if slave is not None:
                os.close(slave)
    elif device in ("EPIPE","EPIPE-ignore"):
        read,write = os.pipe()
        os.close(read)
        try:
            p = subprocess.run([invocation,*args],stdout=write,restore_signals=device=="EPIPE",**kw)
        finally:
            os.close(write)
        out = b""
    elif device == "ferme":
        p = subprocess.run([invocation,*args],preexec_fn=lambda: os.close(1),**kw)
        out = b""
    elif device == "EBADF":
        with open(os.devnull,"rb") as fd:
            p = subprocess.run([invocation,*args],stdout=fd,**kw)
        out = b""
    else:
        p = subprocess.run([invocation,*args],stdout=subprocess.PIPE,**kw)
        out = p.stdout
    return p.returncode,out,p.stderr

def cases():
    paths = [b"",b".",b"..",b"single",b"-",b"/",b"//",b"///",b"//a//",b"///a///",b"///a///b",b"///a//b/",b"dir/file",b"abc///",b"/absent/deep/name/",b"a//b/c",b"/a//.//",b"a/../b",b"a b/f",b"lf\nx/f",b"\xff\xfe/f",b"\\a\\b"]
    for i,path in enumerate(paths):
        yield f"chemin-{i}",[path],{},"flux",None
    for opt in [b"-z",b"-zz",b"--z",b"--ze",b"--zer",b"--zero"]:
        yield "nul-"+opt.decode(),[opt,*paths],{},"flux",None
    for i,args in enumerate([[],[b"--"],[b"-z"],[b"one/x",b"-z",b"two/y"],[b"a",b"--",b"--zero",b"--help",b"--version",b"--"],[b"-zz",b"--zero",b"-z",b"a/b"],[b"-h"],[b"-v"],[b"-zzq"],[b"---"],[b"--="],[b"--=x"],[b"--bad"],[b"foo",b"--zerox"],[b"--help=x"],[b"--ver="],[b"--z=x"],[b"-\xff"],[b"--\xff"],[b"-z=2"],[b"--bad",b"--help"]]):
        yield f"grammaire-{i}",args,{},"flux",None
    yield "posix",[b"a/b",b"-z",b"--version"],{"POSIXLY_CORRECT":"1"},"flux",None
    for name in ["help","version"]:
        for n in range(1,len(name)+1):
            yield "prefixe-"+name[:n],[b"a/b",b"-zz",("--"+name[:n]).encode(),b"--invalid"],{},"flux",name
    yield "priorite-version",[b"--version",b"--help"],{},"flux","version"
    yield "priorite-aide",[b"--help",b"--version"],{},"flux","help"
    yield "invocation-option",[b"--bad"],{"ARGV0_TEST":"/un/chemin/alias"},"flux",None
    yield "invocation-manquant",[],{"ARGV0_TEST":"/un/chemin/alias"},"flux",None
    for tag in ["fr","Fr-Latn-CA","fr-x-demo","fr--CA","frr","en-US","C"]:
        for name in ["help","version"]:
            yield name+"-"+tag,[("--"+name).encode()],{"LC_ALL":tag},"flux",name
        yield "chemins-"+tag,paths,{"LC_ALL":tag},"flux",None
    for style in STYLES:
        sjis=[s.encode("shift_jis")+b"/f" for s in ["表","ソ","表'","表a","×","字"]]
        yield "sjis-"+style,sjis,{"LC_ALL":"ja_JP.SJIS","QUOTING_STYLE":style},"pty",None
        yield "latin1-"+style,[b"a\xe9'/f",b"a\xff/y",b"x\x85/t"],{"LC_ALL":"en_US.ISO8859-1","QUOTING_STYLE":style},"pty",None
    for mode in ["flux","pty"]:
        for lang in ["C","en_US.UTF-8"]:
            for style in [None,*STYLES,"s","lit","cl","unknown"]:
                env={"LC_ALL":lang}
                if style is not None:
                    env["QUOTING_STYLE"]=style
                names=[b"plain",b"q name",b"i'j",b"x'y$z",b"a\n\r\tb",b"\x01\x07\xff",b"a\\b",b"a\"b","é".encode(),"d’x".encode(),b"=",b"{",b"#name",b"a~b",b"a']b",b"~a'b","a'é".encode()]
                args=[s+b"/end" for s in names]
                yield f"citation-{mode}-{lang}-{style}",args,env,mode,None
                if mode=="pty":
                    yield f"nul-tty-{lang}-{style}",[b"-z",*args],env,mode,None
    rng=random.Random(27024)
    for i in range(20):
        alphabet=b"ab./\n \xff-"
        args=[b"--",*(bytes(rng.choice(alphabet) for _ in range(rng.randrange(60))) for _ in range(25))]
        yield f"lot-{i}",args,{},"flux",None
    for device in ["EBADF","EPIPE","ferme","EPIPE-ignore"]:
        for opt in [b"a/b",b"--zero",b"--help",b"--version"]:
            yield device+opt.decode(),[opt,b"a/b"],{},device,None

def main(dest):
    dest.parent.mkdir(parents=True,exist_ok=True)
    version = subprocess.check_output([GNU,"--version"])
    assert version.startswith(b"dirname (GNU coreutils) 9.12\n")
    table = []
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as temp:
        locked = Path(temp)/"interdit"
        locked.mkdir(mode=0)
        try:
            tests = [*cases(),("interdit",[os.fsencode(locked/"child"/"file")],{},"flux",None)]
            for ident,args,env,device,doc in tests:
                g=invoke(GNU,args,env,device)
                n=invoke(NOVA,args,env,device)
                # Le préfixe diagnostique GNU provient du nom installé par Homebrew.
                err=g[2].replace(b"gdirname: ",b"dirname: ",1)
                ok=n==(g[0],g[1],err)
                if device=="pty":
                    assert g[1] and n[1], ident
                if doc:
                    french=env.get("LC_ALL") in ("fr","Fr-Latn-CA","fr-x-demo")
                    expected=("Afficher la partie répertoire" if french else "Print the directory part").encode()
                    ok=(n[0]==g[0]==0 and n[2]==g[2]==b"" and
                        (n[1]==b"dirname (nova-dirname) 0.24.0\n" and b"9.12" in g[1] if doc=="version"
                         else all(s in n[1] for s in (expected,b"-z",b"--zero",b"--help",b"--version",b"man dirname")) and b"--zero" in g[1]))
                def record(x):
                    return dict(statut=x[0],out=x[1].hex(),err=x[2].hex())
                table.append(dict(id=ident,argv=[s.hex() for s in args],env=env,mode=device,doc=doc,gnu=record(g),nova=record(n),ok=ok))
        finally:
            locked.chmod(0o700)
    dest.write_text(json.dumps(table,ensure_ascii=False,separators=(",",":"))+"\n")
    bad=[row for row in table if not row["ok"]]
    print(f"{len(table)-len(bad)}/{len(table)} cas conformes")
    for row in bad[:15]:
        print(row["id"], row["gnu"],row["nova"])
    if bad:
        raise SystemExit(1)

if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("sortie",type=Path)
    main(parser.parse_args().sortie)
