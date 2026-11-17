"""Oracle Darwin indépendant et consommation effective du pipe, sans les substitutions GNU."""
import hashlib
import json
import os
import platform
import subprocess as sp
from options import GNU, NOVA, invoke

data = os.uname()
fields = [os.fsencode(x) for x in (data.sysname,data.nodename,data.release,data.version,data.machine)]
cpu = sp.check_output(["/usr/bin/uname","-p"]).rstrip(b"\n")
expected = [*fields,cpu,b"unknown",b"Darwin"]
rows = []
assert platform.machine()=="arm64"
for flag,value in zip("snrvmpio",expected):
    for binary in [GNU,NOVA]:
        status,out,err=invoke(binary,["-"+flag])
        assert (status,out,err)==(0,value+b"\n",b""),(binary,flag)
    rows.append(dict(option="-"+flag,octets=len(value)+1,sha256=hashlib.sha256(value+b"\n").hexdigest()))

for binary in [GNU,NOVA]:
    # Une entrée ouverte sans données ne doit pas retarder la sortie.
    with sp.Popen(["uname","-A"],executable=binary,env={"LC_ALL":"C"},stdin=sp.PIPE,stdout=sp.PIPE,stderr=sp.PIPE) as producer:
        with sp.Popen(["/usr/bin/wc","-l"],stdin=producer.stdout,stdout=sp.PIPE) as consumer:
            producer.stdout.close()
            assert producer.wait(timeout=3)==0 and producer.stderr.read()==b""
            count=consumer.communicate(timeout=3)[0]
            assert consumer.returncode==0 and int(count)==7

print(json.dumps(dict(champs=rows,pipe="uname -A | wc -l",lignes=7,stdin="ouvert sans données"),ensure_ascii=False,indent=2))
