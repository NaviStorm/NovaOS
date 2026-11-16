"""Longues lignes, reprise de contexte sur disque et pics RSS du processus grep."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

exes = ["/opt/homebrew/opt/grep/libexec/gnubin/grep", sys.argv[1]]
env = dict(os.environ, LC_ALL="C", LANG="C")
version = subprocess.check_output([exes[0],"--version"],env=env).decode()
assert version.startswith("grep (GNU grep) 3.12\n")
pcre = subprocess.run([exes[0],"-Po","(?<=a)b"],input=b"ab\n",capture_output=True,env=env)
assert (pcre.returncode,pcre.stdout,pcre.stderr) == (0,b"b\n",b"")
rows = []
with tempfile.TemporaryDirectory(dir=env["TMPDIR"]) as tmp:
    root = Path(tmp)
    path = root/"temoin"
    for case, size, args in [
        ("longue-ERE",8,["-Ec","x.*Z"]), ("longue-ERE",64,["-Ec","x.*Z"]),
        ("longue-BRE",16,["-Gc","^xx*Z$"]),
        ("longue-PCRE",16,["-Pc","(?<=x)Z$"]),
        ("longue-NUL",16,["-zEc","x.*Z$"]),
        ("longue-absente",16,["-Ec","Y.*Z"]),
        ("longue-F",64,["-Fc","Z"]), ("longue-fichier-texte",16,["-aFn","Z"]),
        ("contexte-disque",32,["-B2300","FIN"]), ("contexte-rotation",64,["-B2300","FIN"]),
    ]:
        block = (b"a"*4095+b"\n")*256 if case.startswith("contexte") else b"x"*(1024*1024)
        with path.open("wb") as f:
            for _ in range(size):
                f.write(block)
            f.write(b"FIN\n" if case.startswith("contexte") else b"Z")
        pair = []
        for exe in exes:
            out = root/"stdout"
            with out.open("wb") as f:
                p = subprocess.run(["/usr/bin/time","-lp",exe,*args,path],env=env,stdout=f,stderr=subprocess.PIPE,timeout=90)
            assert p.returncode == (1 if case == "longue-absente" else 0), (exe,args,p.stderr)
            with out.open("rb") as f:
                digest = hashlib.file_digest(f,"sha256").hexdigest()
            usage = p.stderr.decode()
            pair.append(dict(octets=out.stat().st_size,sha256=digest,
                rss_octets=int(re.search(r"(\d+)\s+maximum resident set size",usage)[1]),
                secondes=float(re.search(r"real\s+([\d.]+)",usage)[1]),statut=p.returncode))
        equal = pair[0]["sha256"] == pair[1]["sha256"] and pair[0]["octets"] == pair[1]["octets"]
        rows.append(dict(cas=case,mio=size,args=args,gnu=pair[0],candidat=pair[1],identique=equal))
Path(sys.argv[2]).write_text(json.dumps(dict(oracle=version,locale="C",pcre=pcre.stdout.hex(),mesures=rows),indent=1)+"\n")
assert all(r["identique"] for r in rows)
