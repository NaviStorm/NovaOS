"""Contrôler la position de stdin, les erreurs de lecture et les fermetures réelles."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT=Path(__file__).resolve().parents[1]
TOOLS=["/opt/homebrew/bin/gtr",str(ROOT/"bin/tr")]
def execute(binary,args,fd,**kwargs):
    return subprocess.run(["tr",*args],executable=binary,stdin=fd,stderr=subprocess.PIPE,
                          env=dict(os.environ,LC_ALL="C"),timeout=5,**kwargs)

def main():
    result=[]
    for binary in TOOLS:
        for args,code in [(["--help"],0),(["--version"],0),(["--foo"],1),(["-ds"],1)]:
            with tempfile.TemporaryFile(dir=os.environ["NOVAOS_TMPDIR"]) as fd:
                fd.write(b"aucun octet lu");fd.seek(0)
                p=execute(binary,args,fd,stdout=subprocess.PIPE)
                assert p.returncode==code and fd.tell()==0
                result.append(dict(tool=binary,args=args,position=fd.tell(),status=code,stdout=p.stdout.hex(),stderr=p.stderr.hex()))
        fd=os.open(".",os.O_RDONLY)
        try:
            p=execute(binary,["a","b"],fd,stdout=subprocess.PIPE)
            assert p.returncode==1 and p.stderr.endswith(b": read error: Is a directory\n") and not p.stdout
            result.append(dict(tool=binary,error="EISDIR",status=1,stderr=p.stderr.hex()))
        finally: os.close(fd)
        for args in [["a","b"],["--help"],["--version"]]:
            rd,wr=os.pipe();os.close(rd)
            try:
                with tempfile.TemporaryFile(dir=os.environ["NOVAOS_TMPDIR"]) as fd:
                    fd.write(b"a"*512);fd.seek(0)
                    p=execute(binary,args,fd,stdout=wr)
                    assert p.returncode==-13 and not p.stderr,(binary,args,p)
                    result.append(dict(tool=binary,args=args,error="SIGPIPE",status=p.returncode))
            finally: os.close(wr)
    for lang,french in [("fr",True),("fr-CA",True),("FR-Latn-FR",True),("fr--CA",False),("en-US",False)]:
        p=subprocess.run([TOOLS[1],"--help"],stdin=subprocess.DEVNULL,capture_output=True,env=dict(os.environ,LANG=lang))
        assert not p.returncode and (b"Traduire" if french else b"Translate") in p.stdout
        result.append(dict(lang=lang,help=p.stdout.hex(),status=p.returncode))
    Path(sys.argv[1]).write_text(json.dumps(result,indent=2)+"\n")

if __name__=="__main__":main()
