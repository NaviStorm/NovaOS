"""Relève les statuts des deux processus et les positions de stdin des appels sans lecture."""
import json
import os
import subprocess
import tempfile
from pathlib import Path
from laboratoire import HERE,TMP,parser,tools
p=parser(__doc__)
a=p.parse_args()
bins=tools(a)
observations=[]
with tempfile.TemporaryDirectory(dir=TMP,prefix="cmp-flux-") as folder:
    path=Path(folder)
    data=bytes(range(256))*32768
    (path/"a").write_bytes(data)
    for name,binary in bins.items():
        env={**os.environ,"LC_ALL":"C","LANG":"C"}
        for opt,rc in [("--help",0),("--version",0),("--inconnue",2)]:
            with (path/"a").open("rb") as inp:
                inp.seek(17)
                result=subprocess.run(["cmp",opt],executable=binary,env=env,stdin=inp,capture_output=True)
                assert result.returncode==rc and inp.tell()==17
                observations.append(dict(outil=name,option=opt,statut=rc,stdin_avant=17,stdin_apres=inp.tell()))
        for equal in [True,False]:
            (path/"b").write_bytes(data if equal else b"x"+data[1:])
            with subprocess.Popen(["/bin/cat","a"],cwd=path,env=env,stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE) as cat:
                reader=subprocess.run(["cmp","-s","b","-"],cwd=path,env=env,executable=binary,
                                      stdin=cat.stdout,capture_output=True)
                cat.stdout.close()
                writer=cat.wait(timeout=10)
                assert reader.returncode==int(not equal) and reader.stdout==reader.stderr==b""
                assert (writer==0)==equal
                observations.append(dict(outil=name,identiques=equal,cmp=reader.returncode,cat=writer,
                                         stderr_cat=cat.stderr.read().hex(),octets_source=len(data)))
if a.json:a.json.write_text(json.dumps(observations,indent=2)+"\n")
print(len(observations),"observations vérifiées")
