"""Fermer le consommateur, puis comparer le signal et les octets du diagnostic."""
import json
import os
import subprocess
import sys
from options import GNU,NOVA
def run(exe,restore):
    read,write=os.pipe()
    os.close(read)
    try:
        proc=subprocess.run(["realpath","-msz",*(["/a/b/../c"]*3072)],executable=exe,
                            env={"LC_ALL":"C"},stdin=subprocess.DEVNULL,stdout=write,
                            stderr=subprocess.PIPE,restore_signals=restore)
        return dict(statut=proc.returncode,err=proc.stderr.hex())
    finally:os.close(write)
rows=[]
for restore in [True,False]:
    g,n=(run(exe,restore) for exe in [GNU,NOVA])
    rows.append(dict(sigpipe_defaut=restore,gnu=g,nova=n))
    assert g==n,(g,n)
print(json.dumps(rows,indent=1))
