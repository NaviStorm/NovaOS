from pathlib import Path
import argparse
import hashlib
import os
import subprocess

HERE = Path(__file__).resolve().parent
TMP = Path(os.environ.get("NOVAOS_TMPDIR", HERE.parents[1]/"tmp")).resolve()
SEED = 270240
def parser(description):
    p = argparse.ArgumentParser(description=description)
    p.add_argument("--macOS", action="store_true", help="exécuter GNU cmp 3.12")
    p.add_argument("--novaOS", action="store_true", help="exécuter le Mach-O Nova")
    p.add_argument("--json", type=Path, help="enregistrer toutes les observations")
    return p
def tools(args):
    TMP.mkdir(parents=True,exist_ok=True)
    os.environ.update(TMPDIR=str(TMP)+"/",NOVAOS_TMPDIR=str(TMP))
    both = not (args.macOS or args.novaOS)
    result = {}
    if both or args.novaOS:
        result["Nova"] = Path(os.environ.get("NOVA_CMP", HERE/"bin/cmp")).resolve()
    if both or args.macOS:
        result["GNU"] = Path(os.environ.get("GNU_CMP",HERE/"reference-gnu/bin/cmp")).resolve()
    for name,path in result.items():
        ver=subprocess.check_output([path,"--version"],env={**os.environ,"LC_ALL":"C"}).splitlines()[0]
        if name=="GNU" and ver!=b"cmp (GNU diffutils) 3.12":
            raise ValueError(f"référence incorrecte {path} {ver!r}")
    return result
def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f,"sha256").hexdigest()
