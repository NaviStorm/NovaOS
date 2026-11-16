"""Débit de bout en bout sur fichiers Darwin avec validation après chaque passage."""
import argparse
import collections
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess as sp
import tempfile
import time
from options import GNU,NOVA,digest

def tasks():
    names=sp.check_output(["ps","-Aco","comm="],text=True).lower().splitlines()
    return dict(collections.Counter(Path(s.strip()).name for s in names if any(k in s for k in ("qemu-system","rustc","cargo","gtee","python"))))

def main():
    cli=argparse.ArgumentParser(description=__doc__)
    cli.add_argument("profil",choices=("court","complet"),default="court",nargs="?")
    cli.add_argument("rapport",type=Path,nargs="?")
    args=cli.parse_args()
    rows=[]
    count=3 if args.profil=="court" else 7
    sizes=[1,8] if args.profil=="court" else [1,16,64,256]
    meta=dict(machine=platform.platform(),passages=count,charge_debut=tasks(),methode="fichiers ordinaires, caches chauds, sans fsync, lancement compris")
    print("Taille ou nombre | Traitement                   | Nova tee (ms) | GNU tee (ms)")
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as folder:
        root=Path(folder)
        corpus=root/"corpus"
        for mib in sizes:
            block=random.Random(27).randbytes(1024**2)
            with corpus.open("wb") as f:
                for _ in range(mib): f.write(block)
            from hashlib import file_digest
            with corpus.open("rb") as f: expected=file_digest(f,"sha256").digest()
            for total in (1,4):
                dest=[root/("sortie"+str(i)) for i in range(total)]
                sample={"Nova":[],"GNU":[]}
                for n in range(count+1):
                    variants=[("GNU",GNU),("Nova",NOVA)]
                    for label,exe in variants[::1 if n%2==0 else -1]:
                        for path in dest:path.write_bytes(b"")
                        with corpus.open("rb") as src,dest[0].open("wb") as out:
                            start=time.monotonic_ns()
                            proc=sp.Popen([exe,*dest[1:]],stdin=src,stdout=out,stderr=sp.PIPE)
                            _,status,usage=os.wait4(proc.pid,0)
                            elapsed=time.monotonic_ns()-start
                            proc.returncode=os.waitstatus_to_exitcode(status)
                            assert proc.returncode==0 and proc.stderr.read()==b""
                            proc.stderr.close()
                        for path in dest:
                            with path.open("rb") as f: assert file_digest(f,"sha256").digest()==expected
                        if n:sample[label].append(elapsed/1e6)
                        rows.append(dict(mio=mib,sorties=total,outil=label,passage=n,echauffement=n==0,
                            ns=elapsed,rss_octets=usage.ru_maxrss,octets_lus=mib*1024**2,
                            octets_ecrits=total*mib*1024**2,debit_entree_Mio_s=mib*1e9/elapsed,
                            debit_sorties_Mio_s=total*mib*1e9/elapsed,sha256=expected.hex()))
                print(f"{mib:>10} Mio | {total} sortie(s), {total*mib:>4} Mio écrits | {statistics.median(sample['Nova']):13.3f} | {statistics.median(sample['GNU']):12.3f}")
    if args.rapport:
        meta.update(charge_fin=tasks(),gnu_sha256=digest(GNU.read_bytes()),nova_sha256=digest(NOVA.read_bytes()),mesures=rows)
        args.rapport.write_text(json.dumps(meta,indent=2)+"\n")
if __name__=="__main__":main()
