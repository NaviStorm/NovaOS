import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
TMP = Path(os.environ.get("NOVAOS_TMPDIR", ROOT.parent / "tmp")).resolve()
ENV = {**os.environ, "LC_ALL":"C", "LANG":"en", "LANGUAGE":"C", "TZ":"UTC0", "TERM":"xterm", "TMPDIR":str(TMP)+"/"}
ENV.pop("POSIXLY_CORRECT", None)
NOVA = HERE / "bin/diff"
def oracle():
    candidates = [os.environ.get("GNU_DIFF"), shutil.which("gdiff"), TMP/"diff-gnu-3.12/bin/diff", TMP/"cmp-0240/gnu/bin/diff"]
    for p in filter(None, candidates):
        if Path(p).is_file() and b"(GNU diffutils) 3.12\n" in subprocess.check_output([p,"--version"],env=ENV):
            return Path(p).resolve()
    raise SystemExit("Définir GNU_DIFF avec le chemin absolu de GNU diffutils 3.12.")
def digest(path):
    with Path(path).open("rb") as f:
        return hashlib.file_digest(f,"sha256").hexdigest()
def execute(binary,args,cwd,stdin=b"",env=None):
    r=subprocess.run(["diff",*args],executable=binary,cwd=cwd,input=stdin,capture_output=True,env={**ENV,**(env or {})},timeout=30)
    return [r.returncode,r.stdout.hex(),r.stderr.hex()]
def parse(description):
    p=argparse.ArgumentParser(description=description)
    mode=p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--rapide",action="store_true",help="petit corpus de vérification")
    mode.add_argument("--complet",action="store_true",help="campagne complète")
    p.add_argument("--sortie",type=Path,help="répertoire de résultats")
    return p.parse_args()
def save(path,data):
    path.write_text(json.dumps(data,ensure_ascii=False,indent=1)+"\n")
def environment():
    g=oracle()
    return dict(host=platform.platform(),machine=platform.machine(),gnu=str(g),gnu_sha256=digest(g),nova_sha256=digest(NOVA),locale="C",tz="UTC0",os=subprocess.check_output(["sw_vers"],text=True).strip(),source=subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip())
