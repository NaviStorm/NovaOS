"""Deux travailleurs SHA-256 et un écrivain APFS privé pendant une série complète."""
import argparse
import hashlib
import json
import multiprocessing as mp
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def compete(kind, stop, ready, report, folder):
    block = bytes(1024**2)
    name = mp.current_process().name
    count = 0
    start = time.monotonic()
    if kind == "calcul":
        ready.set()
        while not stop.is_set():
            hashlib.sha256(block).digest()
            count += 1
    else:
        with (folder/"charge.bin").open("w+b",buffering=0) as f:
            ready.set()
            while not stop.is_set():
                f.seek(count % 8 * len(block))
                f.write(block)
                count += 1
                stop.wait(.0125)
    report.put(dict(nom=name,mio=count,secondes=time.monotonic()-start))


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("rapport_charge",type=Path)
    p.add_argument("arguments_comparer",nargs=argparse.REMAINDER)
    args=p.parse_args()
    ctx=mp.get_context("spawn")
    stop=ctx.Event()
    queue=ctx.Queue()
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as tmp:
        workers=[]
        try:
            for i,kind in enumerate(("calcul","calcul","ecriture")):
                ready=ctx.Event()
                worker=ctx.Process(target=compete,name=f"{kind}-{i}",args=(kind,stop,ready,queue,Path(tmp)))
                worker.start()
                workers.append(worker)
                if not ready.wait(15):
                    raise RuntimeError("Travailleur indisponible")
            campaign = subprocess.run([sys.executable,"-B",Path(__file__).with_name("comparer.py"),
                            *args.arguments_comparer],check=False)
        finally:
            stop.set()
            for w in workers:
                w.join()
        if any(w.exitcode for w in workers):
            raise RuntimeError("Arrêt anormal d’un travailleur")
        with args.rapport_charge.open("x") as f:
            json.dump([queue.get(timeout=5) for w in workers],f,indent=2)
            f.write("\n")
        raise SystemExit(campaign.returncode)
