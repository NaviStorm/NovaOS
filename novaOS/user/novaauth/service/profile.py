"""Prépare une copie de compilation du commit, avec le raccord NovaAuth réservé à QEMU."""
import argparse
from pathlib import Path
import hashlib
import re
import subprocess
import tarfile

def prepare(destination):
    source = Path(__file__).resolve().parents[4]
    revision = subprocess.check_output(["git","rev-parse","HEAD"],cwd=source,text=True).strip()
    destination.mkdir(parents=True,exist_ok=False)
    # Les sources reproductibles viennent du commit désigné. Aucun index de
    # worktree et aucun fichier personnel ou ignoré n'entre dans cette copie.
    with subprocess.Popen(["git","archive",revision,"novaOS","novaFS","components"],cwd=source,stdout=subprocess.PIPE) as archive:
        with tarfile.open(fileobj=archive.stdout,mode="r|") as stream:
            stream.extractall(destination,filter="data")
        if archive.wait()!=0: raise RuntimeError("export Git incomplet")
    patch=destination/"novaOS/user/novaauth/service/raccord.patch"
    for path, preimage in re.findall(r"^diff --git a/(\S+) b/\S+\nindex ([0-9a-f]{40})\.\.[0-9a-f]{40}",patch.read_text(),re.M):
        content=(destination/path).read_bytes()
        oid=hashlib.sha1(b"blob "+str(len(content)).encode()+b"\0"+content).hexdigest()
        if oid!=preimage: raise ValueError(f"source différente de la référence du raccord: {path}")
    subprocess.run(["git","apply","--unidiff-zero","--check",str(patch)],cwd=destination,check=True)
    subprocess.run(["git","apply","--unidiff-zero",str(patch)],cwd=destination,check=True)
    print(revision,destination)
if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination",type=Path)
    prepare(parser.parse_args().destination.resolve())
