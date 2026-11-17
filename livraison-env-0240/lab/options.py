"""Différentiel en octets. Le processus sous test reçoit un environnement fermé."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

BASE = Path(__file__).resolve().parents[1]
ROOT = BASE.parent
GNU = Path("/opt/homebrew/opt/coreutils/libexec/gnubin/env")
NOVA = BASE / "bin/env-darwin-arm64"
WITNESS = BASE / "bin/env-witness-darwin-arm64"
SYNTH = {"LC_ALL":"C","ENV_TEST":"hérité","ENV_OTHER":"avant"}

def sha(p):
    with open(p,"rb") as f:
        return hashlib.file_digest(f,"sha256").hexdigest()

def cases(work,full):
    sample=work/"donnees"
    sample.write_bytes(b"A=1\0informe\0\0A=2\0=vide\0ENV_TEST=lu\0")
    bad=work/"fin-invalide"
    bad.write_bytes(b"A=x")
    denied=work/"interdit"
    denied.touch(mode=0)
    child=str(WITNESS)
    basics=[
        ("héritage",[]),("vide",["-"]),("vide",["-i"]),("vide",["--ignore-environment"]),
        ("affectations",["-i","--","B=1","A=2","B=3","=vide","X="]),
        ("ordre",["-u","ENV_TEST","ENV_TEST=après"]),("suppression",["--unset=ENV_OTHER"]),
        ("suppression",["-uABSENT"]),("nul",["-0"]),("nul",["--null","V=a\nb"]),
        ("stdin",["--env0-from=-","-0"]),("fichier",["--env0-from",str(sample)]),
        ("brut",["-i0","--env0-from",str(sample)]),("fusion",["--env0-from",str(sample),"-uA","A=t"]),
        ("témoin",[child,"23","--help","a\nb",""]),
        ("argv0",["-a","alias",child]),("argv0",["--argv0=",child]),
        ("cwd",["-C",str(work),child]),("cwd",["--chdir="+str(work),child]),
        ("enfant combiné",["-iaTEMOIN","--chdir="+str(work),"LANG=fr-CA","ENV_TEST=42",child,"42"]),
        ("PATH",["-i","PATH="+str(WITNESS.parent),WITNESS.name,"7"]),
        ("débogage",["--debug","--unset=ENV_OTHER","ENV_TEST=nouveau",child]),
        ("découpage",["-vS","-i A='\x24{ENV_TEST}' B=\x24{ENV_TEST} # fin"]),
        ("découpage",["--split-string",r'''-i X="a\_b"\_Y='c d' ''']),
        ("découpage",["-S","-S '-i A=b'","C=d"]),
        ("délimiteur",["--","-","ENV_TEST=1"]),
        ("fin des options",["X=1","--help"]),("commande absente",["env-arc027-absolument-absent"]),
        ("accès exécution",[str(denied)]),("répertoire inexistant",["-C",str(work/"absent"),child]),
        ("erreur lecture",["--env0-from="+str(denied)]),("NUL manquant",["--env0-from="+str(bad)]),
        ("chemin absent",["--env0-from="+str(work/"absent")]),("dernière source",["--env0-from=/absent","--env0-from="+str(sample)]),
    ]
    for opt in ["--block-signal","--default-signal","--ignore-signal"]:
        for suffix in ["","=","=USR1","=sigusr1,13","=130","=FAUX","=0"]:
            basics.append(("signal", [opt+suffix,"--list-signal-handling",child]))
    basics += [("masque combiné",["-v","--block-signal=USR1","--ignore-signal=USR1","--list-signal-handling",child]),
               ("réinitialisation",["--block-signal=USR1","--ignore-signal=USR1","--default-signal=USR1",child]),
               ("liste",["--list-signal-handling",child]),("SIGKILL",["--ignore-signal=KILL",child])]
    if full:
        for a in [["-u"],["-a"],["-S"],["-C"],["--env0-from"],["--argv0"],["--unset="],
                  ["-u","x=y"],["-iu","x=y"],["--null",child],["-a","arg"],["-C","."],
                  ["--debug=x"],["--i"],["--inconnu"],["-z"],["-iv"],["--igN"],["--nul"],
                  ["--env0-f",str(sample)],["-S","'"],["-S","\\"],["-S",r"A=\y"],
                  ["-S","X=$T"],["-S",r'''A="\c"'''],["-S","-i \x24{ABSENT} A=B"],
                  ["-0","OCTET=".encode()+b"\xff"],["-S",r"-i A=a\cb"],["-i","A=b c"],["--he"],["--ver"]]:
            basics.append(("grammaire",a))
        for c in [r'\t',r'\n',r'\v',r'\r',r'\f',r'\$',r'\#',r'\"',r"\'",r'\\',r'\_']:
            basics.append(("échappement",["-i0S","A=\"x"+c+"y\""]))
    for label,args in basics: yield label,args,SYNTH
    for lang in [None,"fr-CA","fr-Zzzz","fr-zzzzz","en-GB"]:
        for opt in ["--help","--version"]:
            yield "document", [opt], {} if lang is None else {"LANG":lang}

def run(binary,args,env,cwd=None):
    p=subprocess.run(["env",*args],executable=binary,env=env,cwd=cwd,
                     input=b"A=1\0brut\0A=2\0",capture_output=True,timeout=8)
    return dict(status=p.returncode,stdout=p.stdout.hex(),stderr=p.stderr.hex())

def additional_cases(work):
    """Exercer un vrai PTY, un pipe de deux processus, un EPIPE et un shebang."""
    import errno
    import tty
    def packed(status,out=b"",err=b""):
        return dict(status=status,stdout=out.hex(),stderr=err.hex())
    for mode in ["literal","c","c-maybe","escape","locale","clocale","shell","shell-always","shell-escape","shell-escape-always","incorrect"]:
        for locale in ["C","en_US.UTF-8"]:
            env={"LC_ALL":locale,"QUOTING_STYLE":mode}
            args=["-i","X=a b","Y=\n","Z=","É=é","T='","B=\\"]
            def terminal(binary):
                master,slave=os.openpty()
                try:
                    tty.setraw(slave)
                    proc=subprocess.run(["env",*args],executable=binary,env=env,stdout=slave,stderr=subprocess.PIPE,timeout=8)
                    os.set_blocking(master,False)
                    data=bytearray()
                    while True:
                        try: chunk=os.read(master,8192)
                        except OSError as e:
                            if e.errno in (errno.EIO,errno.EAGAIN):break
                            raise
                        if not chunk:break
                        data.extend(chunk)
                    return packed(proc.returncode,data,proc.stderr)
                finally:
                    os.close(master)
                    if slave>=0:os.close(slave)
            yield "PTY "+mode,args,env,terminal(GNU),terminal(NOVA)
    args=["-0i","X=a\nb","Z="]
    def pipeline(binary):
        with subprocess.Popen(["env",*args],executable=binary,env=SYNTH,stdout=subprocess.PIPE,stderr=subprocess.PIPE) as writer:
            reader=subprocess.run(["env","-i0","--env0-from=-"],executable=binary,env=SYNTH,stdin=writer.stdout,capture_output=True,timeout=8)
            writer.stdout.close()
            assert writer.wait(8)==reader.returncode==0 and reader.stdout==b"X=a\nb\0Z=\0" and not writer.stderr.read()
            return packed(reader.returncode,reader.stdout,reader.stderr)
    yield "pipe NUL",args,SYNTH,pipeline(GNU),pipeline(NOVA)
    def broken(binary):
        r,w=os.pipe();os.close(r)
        try:
            p=subprocess.run(["env",*args],executable=binary,env=SYNTH,stdout=w,stderr=subprocess.PIPE,timeout=8)
            return packed(p.returncode,err=p.stderr)
        finally:os.close(w)
    yield "lecteur fermé",args,SYNTH,broken(GNU),broken(NOVA)
    script=work/"exemple"
    def shebang(binary):
        script.write_bytes(b"#!"+os.fsencode(binary)+b" -S /bin/echo 'deux mots'\n")
        script.chmod(0o700)
        p=subprocess.run([script,"suite"],env=SYNTH,capture_output=True,timeout=8)
        assert p.returncode==0 and b"deux mots" in p.stdout
        return packed(p.returncode,p.stdout,p.stderr)
    yield "shebang Darwin",["-S","/bin/echo 'deux mots'"],SYNTH,shebang(GNU),shebang(NOVA)
    for args in [["-iq"],["-i z"],["-i\tz"],["--env0-from",str(work/"absent")],["--ignore-signal=SIG13",str(WITNESS)],
                 ["-vS","-i ENV_TEST=\"\x24{ENV_TEST}\""],["--block-signal=,sig2,,",str(WITNESS)],
                 ["--ignore-signal=POLL"],["-u","x=y"],["-u","ENV_TEST","B=1","Z=2","Z=3"]]:
        for locale in ["C","en_US.UTF-8"]:
            env=dict(SYNTH,LC_ALL=locale,QUOTING_STYLE="incorrect")
            yield "diagnostic ou effet "+locale,args,env,run(GNU,args,env,work),run(NOVA,args,env,work)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode",choices=["rapide","complet"])
    parser.add_argument("out",type=Path)
    a=parser.parse_args()
    rows=[]
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"],prefix="env-options-") as tmp:
        for label,args,e in cases(Path(tmp),a.mode=="complet"):
            g=run(GNU,args,e,tmp); n=run(NOVA,args,e,tmp)
            doc=args and args[0] in ["--he","--ver","--version","--help"]
            if doc:
                version=args[0] in ["--version","--ver"]
                fr=e.get("LANG") in ["fr-Zzzz","fr-CA"]
                marker=(b"env (NovaOS) 0.24.0\n" if version else ("Afficher un environnement" if fr else "Display an environment").encode())
                same=g["status"]==n["status"]==0 and marker in bytes.fromhex(n["stdout"]) and not n["stderr"]
            else: same=n==g
            rows.append(dict(cas=label,arguments=[os.fsencode(v).hex() for v in args],environnement=e,GNU=g,Nova=n,
                             oracle="ARC-026" if doc else "GNU octets et statut",ok=same))
        if a.mode=="complet":
            for label,args,e,g,n in additional_cases(Path(tmp)):
                rows.append(dict(cas=label,arguments=[os.fsencode(v).hex() for v in args],environnement=e,GNU=g,Nova=n,
                                 oracle="GNU octets et statut",ok=n==g))
    a.out.mkdir(parents=True,exist_ok=True)
    (a.out/"options.json").write_text(json.dumps(rows,ensure_ascii=False,indent=1)+"\n")
    for r in rows:
        if not r["ok"]:
            print(r["cas"],[bytes.fromhex(v) for v in r["arguments"]])
            for k in ["GNU","Nova"]:
                v=r[k];print(k,v["status"],bytes.fromhex(v["stdout"])[:160],bytes.fromhex(v["stderr"])[:350])
    print(f"{sum(r['ok'] for r in rows)}/{len(rows)} cas validés")
    return int(any(not r["ok"] for r in rows))
if __name__=="__main__": raise SystemExit(main())
