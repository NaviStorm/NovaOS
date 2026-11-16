"""Préparer et relire une paire d'objets distincts par taille, sur une image jetable P9."""
import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/"novaOS/tests"))
from cli import HOST
from pipe_natif import host
from preuves_stockage import empreinte
p=argparse.ArgumentParser(description=__doc__)
p.add_argument("secteur",type=int,choices=[512,4096])
p.add_argument("sortie",type=Path)
p.add_argument("--large",action="store_true",help="ajouter 100 Mio, 256 Mio + 1 et 1 Gio logiques")
a=p.parse_args()
a.sortie.mkdir(parents=True,exist_ok=False)
source=a.sortie/"genese.raw"; image=a.sortie/"corpus.raw"
sizes=[1<<20]
if a.large:sizes += [100<<20,(256<<20)+1,1<<30]
commands=[]
for size in sizes:
    for letter in ["a","b"]:
        name=f"/{letter}{size}"
        commands.extend([f"create {name}",f"write {name} {size-1} ab"])
with source.open("xb") as f:
    proc=subprocess.run([HOST,"script","512",str(a.secteur)],stdout=f,stderr=subprocess.PIPE,
                        input=("\n".join(commands)+"\n").encode(),check=True)
fs,lv=re.search(rb"FS=(\w+) LV=(\w+)",proc.stderr).groups()
with image.open("xb") as f:host(source,"migrate-content-p9","512",str(a.secteur),fs,lv,output=f)
host(image,"verify","512",str(a.secteur),fs,lv)
reader=a.sortie/"lire-corpus"
deps=HOST.parent/"deps"
libraries=list(deps.glob("libnovafs-*.rlib"))
if len(libraries)!=1:raise RuntimeError("utiliser un target NovaFS Darwin de release isolé")
subprocess.run(["rustup","run","stable","rustc","--edition=2024","-O","--target","aarch64-apple-darwin",
    "-L","dependency="+str(deps),"--extern","novafs="+str(libraries[0]),
    Path(__file__).with_name("lire-corpus-p9.rs"),"-o",reader],check=True)
rows=[]
for size in sizes:
    expected=hashlib.sha256()
    block=bytes(1<<20)
    for offset in range(0,size-1,len(block)):expected.update(block[:min(len(block),size-1-offset)])
    expected.update(b"\xab")
    for letter in ["a","b"]:
        name=f"/{letter}{size}"
        ident,count=map(int,subprocess.check_output([reader,image,str(a.secteur),fs,lv,name,str(size)]).split())
        assert count==size and all(item["id"]!=ident for item in rows)
        rows.append(dict(nom=name,id=ident,octets=count,sha256=expected.hexdigest()))
(a.sortie/"corpus.json").write_text(json.dumps(dict(image=empreinte(image),secteur=a.secteur,
    fichiers=rows,octets_image=image.stat().st_size),indent=2)+"\n")
