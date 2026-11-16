"""Campagne sur trois fichiers indépendants présentés par un descripteur VMDK plat."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, Future
from queue import Queue
from threading import Barrier

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts"))
from bloc_qemu import command

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def qualify(arch, sector, iso, out):
    out.mkdir(exist_ok=False, parents=True)
    disk, ledger, control = [out / name for name in ("identites.p9", "autorite.raw", "commande.raw")]
    for path, pages in [(disk, 512), (ledger, 12), (control, 1)]:
        with path.open("xb") as stream:
            stream.truncate(pages * 4096)
    descriptor = out / "disque.vmdk"
    descriptor.write_text('# Disk DescriptorFile\nversion=1\nCID=abcdef02\nparentCID=ffffffff\ncreateType="twoGbMaxExtentFlat"\n' +
                          ''.join(f'RW {p.stat().st_size // 512} FLAT "{p.name}" 0\n' for p in (disk,ledger,control)))
    def save(label):
        for p in (disk,ledger):
            shutil.copyfile(p,out / f"{label}-{p.name}")
    def restore(label, *files):
        for p in files:
            shutil.copyfile(out / f"{label}-{p.name}",p)
    rows=[]
    def boot(label, op, cut=False, requests=()):
        packet=(b"NA024LAB" + bytes([op])).ljust(16,b"\0")
        for request,generation in requests:
            packet+=bytes([request])+generation.to_bytes(8,"little")
        control.write_bytes(packet.ljust(4096,b"\0"))
        args=command(arch,sector,descriptor,controls=["-display","none","-monitor","none","-serial","stdio"])
        idx=args.index("-drive")+1
        args[idx]=args[idx].replace("format=raw","format=vmdk")
        env={**os.environ,"NOVA_ARCH":arch,"NOVA_ISO":str(iso),"NOVA_VCPUS":"2"}
        with subprocess.Popen(args,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0) as vm:
            output=bytearray()
            start=None
            end=time.monotonic()+180
            try:
                while time.monotonic()<end and vm.poll() is None:
                    if not select.select([vm.stdout],[],[],1)[0]:continue
                    part=os.read(vm.stdout.fileno(),4096)
                    if not part:break
                    output.extend(part)
                    if start is None and b"NA024 BEGIN" in output:start=time.monotonic_ns()
                    if b"NA024 CUT durable" in output or re.search(rb"NA024 (?:OK|FAIL)[^\r\n]* END",output):break
            finally:
                elapsed=None if start is None else time.monotonic_ns()-start
                if vm.poll() is None:vm.kill()
                vm.wait()
                output.extend(vm.stdout.read())
                (out/f"{label}.serial").write_bytes(output)
        match=re.search(rb"NA024 OK sector=(\d+) heap_delta=(\d+) heap_peak=(\d+) END",output)
        success = (b"NA024 CUT durable" in output if cut else bool(match and int(match[1])==sector))
        rows.append(dict(label=label,commande=op,succes=success,delai_hote_ns=elapsed,
                         variation_tas_octets=int(match[2]) if match else None,
                         pic_tas_supplementaire=int(match[3]) if match else None, p9=digest(disk),autorite=digest(ledger)))
        (out/"resultats.json").write_text(json.dumps(rows,indent=2)+"\n")
        if not success: raise RuntimeError(f"{label}: {output[-1600:]!r}")
        print(arch,sector,label,"OK",flush=True)
        return output

    def race(label,op,epoch):
        # Deux appelants hôte attendent chacun la réponse de l'écrivain invité.
        # Le tampon du banc transporte les deux requêtes en suspens ensemble.
        arrivals=Queue()
        barrier=Barrier(2)
        def caller(rid):
            answer=Future()
            barrier.wait(timeout=10)
            arrivals.put((rid,answer))
            return answer.result(timeout=200)
        with ThreadPoolExecutor(2) as clients:
            futures=[clients.submit(caller,i) for i in (21,22)]
            pending=[arrivals.get(timeout=10) for _ in futures]
            try:
                output=boot(label,op,requests=[(rid,epoch) for rid,_ in pending])
                (winner,_),(loser,_)=pending
                denial="DENIED" if op==14 else "CONFLICT"
                assert f"NA024 REPLIES {winner}=0 {loser}={denial} END".encode() in output
                pending[0][1].set_result("0")
                pending[1][1].set_result(denial)
                assert sorted(f.result() for f in futures)==["0",denial]
            finally:
                for _,answer in pending:
                    if not answer.done():answer.cancel()

    boot("provision",0);save("neuf")
    race("concurrence-enrolement",14,0);restore("neuf",disk,ledger)
    boot("genese-reponse-perdue",1,True)
    boot("genese-retrouvee",3);save("admin")
    restore("neuf",disk,ledger)
    boot("deux-enrolements-admis-un-gagnant",2)
    race("concurrence-creation",15,1);restore("admin",disk,ledger)
    boot("avant-decision",16,True);boot("preparation-invisible",3)
    # Chaque coupure physique part du même état. Une préparation identique
    # déjà sur disque peut légitimement supprimer les écritures et leurs FLUSH.
    restore("admin",disk,ledger)
    boot("entre-bases",4,True);boot("ancien-etat-entier",3)
    boot("decision-sans-reponse",5,True);boot("issue-retrouvee",7);save("alice")
    boot("mauvais-domaine",17);boot("autorite-injoignable",18)
    boot("cycle-revocation",8);save("cycle")
    restore("admin",disk)
    boot("rollback-volume-seul",9)
    restore("admin",ledger)
    boot("rollback-ensemble-limite",3)
    boot("creation-concurrente-cas",6)
    restore("admin",disk,ledger)
    boot("autre-etat-meme-generation",11)
    restore("alice",ledger)
    boot("substitution-refusee",9)
    restore("alice",disk,ledger)
    boot("corruption-contenu",12)
    restore("alice",disk)
    boot("trois-bases-manquantes",10)
    restore("alice",disk)
    ledger.write_bytes(bytes(ledger.stat().st_size))
    boot("autorite-absente",13)
    restore("cycle",disk,ledger)

if __name__=="__main__":
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument("arch",choices=["aarch64","x86_64"])
    ap.add_argument("sector",type=int,choices=[512,4096])
    ap.add_argument("iso",type=Path)
    ap.add_argument("out",type=Path)
    a=ap.parse_args()
    qualify(a.arch,a.sector,a.iso.resolve(strict=True),a.out.resolve())
