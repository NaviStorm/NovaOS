"""Exercer les deux commandes de pipeline de la recette avec le vrai cat de Darwin."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

def main():
    output=Path(sys.argv[1])
    nova=Path(__file__).resolve().parents[1]/"bin/tr"
    gnu="/opt/homebrew/bin/gtr"
    records=[]
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"],prefix="tr-pipe-") as folder:
        source=Path(folder)/"entree"
        data=bytes(range(256))+b"avant"+b" "*(128*41+1)+b"apres\x00\xff"
        source.write_bytes(data)
        for args in [["[:lower:]","[:upper:]"],["-s"," "]]:
            env=dict(os.environ,LC_ALL="C",NOVA_TR_READ_SIZE="128")
            ref=subprocess.run([gnu,*args],input=data,capture_output=True,check=True,env=env)
            with subprocess.Popen(["/bin/cat",source],stdout=subprocess.PIPE,stderr=subprocess.PIPE) as cat:
                with subprocess.Popen([nova,*args],stdin=cat.stdout,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env) as tr:
                    cat.stdout.close()
                    stdout,stderr=tr.communicate()
                cat_err=cat.stderr.read()
                assert cat.wait()==0 and tr.returncode==0 and not cat_err
                assert (stdout,stderr)==(ref.stdout,ref.stderr)
                records.append(dict(args=args,cat_pid=cat.pid,tr_pid=tr.pid,cat_status=0,tr_status=0,
                                    stdin=data.hex(),stdout=stdout.hex(),stderr=stderr.hex(),size=len(stdout)))
        assert source.read_bytes()==data
    output.write_text(json.dumps(dict(source_sha256=hashlib.sha256(data).hexdigest(),runs=records),indent=2)+"\n")

if __name__=="__main__":main()
