"""Comparaison octet par octet des deux flux et du statut, même argv[0]."""
import argparse
import json
import os
from pathlib import Path
import pty
import select
from concurrent.futures import ThreadPoolExecutor
import subprocess
import tempfile
import tty

BASE = Path(__file__).resolve().parents[1]
GNU = Path("/opt/homebrew/bin/greadlink")
NOVA = BASE/"bin/readlink"
STYLES = "literal shell shell-always shell-escape shell-escape-always c c-maybe escape locale clocale".split()

def invoke(binary, args, env, device):
    env = {"LC_ALL":"C", **env}
    invocation=env.pop("ARGV0_TEST","readlink")
    kw = dict(executable=binary,env=env,stderr=subprocess.PIPE)
    if device == "pty":
        master,slave = pty.openpty()
        try:
            tty.setraw(slave)
            # Darwin peut signaler la fin du fils avant la lisibilité du PTY.
            # La sentinelle suit ses octets sur le même esclave, en mode brut.
            marker=b"\0READLINK_FIN_PTY_27024\0"
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

SHORT = "fem nqsvz".replace(" ", "")
LONG = ["canonicalize","canonicalize-existing","canonicalize-missing","no-newline",
        "quiet","silent","verbose","zero","help","version"]

def cases(full, operands):
    for option in [""] + ["-"+c for c in SHORT] + ["--"+n for n in LONG[:-2]]:
        args = [option.encode()] if option else []
        for name in operands:
            yield args+[b"--", name], {}, "flux", None
        yield args+[b"--", *operands], {}, "flux", None
    for c in SHORT:
        for d in SHORT:
            yield [("-"+c+d).encode(), b"chaine", b"absent/inter"], {}, "flux", None
    for args in [[], [b"--"], [b"-H"], [b"-V"], [b"---"], [b"chaine", b"-z"],
                 [b"--", b"--zero"], [b"--="], [b"--ver"], [b"-vs",b"refus/x"]]:
        yield args, {}, "flux", None
    for n in LONG:
        for i in range(1,len(n)+1) if full else [len(n)]:
            prefix = n[:i]
            which = [x for x in LONG if x.startswith(prefix)]
            doc = which[0] if len(which)==1 and which[0] in LONG[-2:] else None
            yield [("--"+prefix).encode(),b"chaine"], {}, "flux", doc
        yield [("--"+n+"=x").encode()], {}, "flux", None
    for e in [{},{"POSIXLY_CORRECT":"1"}]:
        for args in [[b"-q",b"absent"],[b"chaine",b"-z"],[b"-v",b""]]:
            yield args,e,"flux",None
    for style in STYLES+["invalide"]:
        yield [b"--",b"--zero"],{"QUOTING_STYLE":style},"pty",None
        yield [b"-z",b"brut"],{"QUOTING_STYLE":style},"pty",None
    for dev in ["EPIPE","EPIPE-ignore","EBADF","ferme"]:
        yield [b"chaine"],{},dev,None
    for lang in ["fr-CA","fr--CA","fr-Zzzz","en-US"]:
        for doc in LONG[-2:]:
            yield [("--"+doc).encode()],{"LC_ALL":lang},"flux",doc

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode",choices=["rapide","complet"])
    parser.add_argument("rapport",type=Path)
    args=parser.parse_args()
    report=args.rapport.resolve()
    rows=[]
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"],prefix="readlink-cas-") as tmp:
        os.chdir(tmp)
        Path("d").mkdir()
        Path("d/f").touch()
        Path("refus").mkdir()
        os.symlink("interdit","refus/x")
        Path("refus").chmod(0)
        try:
            for src,dst in [(b"./d//../d/f",b"brut"),(b"brut",b"chaine"),(b"inexistant",b"pendant"),
                            (b"boucle",b"boucle"),(b"avec espace\nx",b"--zero"),(b"/",b"racine")]:
                os.symlink(src,dst)
            operands=[b"chaine",b"brut",b"pendant",b"d/f",b"",b"d/f/",b"d/f/..",b"inexistant/",
                      b"absent/inter",b"refus/x",b"refus/..",b"boucle",b"--zero",b"racine/../../",b"//",b"absent x"]
            os.symlink(b"nom\n\xff",b"octets")
            operands.extend([b"octets",b"\xff"])
            for argv,env,dev,doc in cases(args.mode=="complet",operands):
                g=invoke(GNU,argv,env,dev)
                n=invoke(NOVA,argv,env,dev)
                if doc:
                    fr=env.get("LC_ALL") in ("fr-CA","fr-Zzzz")
                    needle=(("Afficher la cible" if fr else "Print a link target").encode()
                            if doc=="help" else b"readlink (nova-readlink) 0.24.0")
                    ok=g[0]==n[0]==0 and not g[2] and not n[2] and needle in n[1]
                else:
                    # Darwin donne à error(3) le nom installé par Homebrew.
                    err=b"\n".join(line.removeprefix(b"g") if line.startswith(b"greadlink: ") else line
                                    for line in g[2].split(b"\n"))
                    ok=(g[0],g[1],err)==n
                def record(v):
                    return dict(statut=v[0],stdout=v[1].hex(),stderr=v[2].hex())
                rows.append(dict(argv=[a.hex() for a in argv],env=env,mode=dev,doc=doc,
                                 gnu=record(g),nova=record(n),ok=ok))
        finally:
            Path("refus").chmod(0o700)
            os.chdir(BASE)
    report.write_text(json.dumps(rows,ensure_ascii=False,indent=1)+"\n")
    print(f"{sum(r['ok'] for r in rows)}/{len(rows)} conformes")
    return int(any(not r["ok"] for r in rows))

if __name__=="__main__": raise SystemExit(main())
