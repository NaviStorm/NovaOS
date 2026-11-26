"""Profil sans NIC, une seule VM à la fois sous le verrou commun NovaOS."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
NOVA = HERE.parents[1]
sys.path.insert(0, str(NOVA / "tests"))
import demarrage

def sha(path):
    return hashlib.file_digest(path.open("rb"), "sha256").hexdigest()

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("archive",type=Path)
    parser.add_argument("architectures",nargs="+",choices=("aarch64","x86_64"))
    parser.add_argument("--phase2b", choices=("mesure","mesure-1","mesure-2","essai","surete","course","rare","rare-1","rare-2"))
    parser.add_argument("--cibles",type=Path,default=NOVA/"target")
    args=parser.parse_args()
    phase = "2b" if args.phase2b else "2"
    complete = b"ETH2B campagne OK" if args.phase2b else b"ETH2 campagne OK"
    root=args.archive.resolve()
    root.mkdir(parents=True,exist_ok=False)
    lock=Path(os.environ["NOVAOS_TMPDIR"]).resolve().parent/".qualification-utilitaires-exclusive.lock"
    owner={"tache":"Ethernet phase "+phase,"pid":os.getpid(),"nonce":uuid.uuid4().hex}
    marker=lock/"owner.json"
    while True:
        try:
            lock.mkdir()
            break
        except FileExistsError:
            print("Attente du verrou commun",flush=True)
            time.sleep(30)
    try:
        marker.write_text(json.dumps(owner))
        for arch in args.architectures:
            run=root/arch
            run.mkdir()
            target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[arch]
            elf=run/"novaos.elf"
            shutil.copyfile(args.cibles/target/"release/novaos",elf)
            iso=run/"ethernet.iso"
            os.environ.update(NOVA_ARCH=arch,NOVA_KERNEL_ELF=str(elf),NOVA_ISO=str(iso),NOVA_RAM="256",NOVA_VCPUS="2" if args.phase2b=="course" else "1")
            with (run/"construction-iso.log").open("w") as log:
                subprocess.run(["sh",NOVA/"scripts/iso.sh",f"ipc-eth2b-{args.phase2b}" if args.phase2b else "ipc-perf",iso],cwd=NOVA,check=True,stdout=log,stderr=subprocess.STDOUT)
            def observe(vm,directory,serial):
                deadline=time.monotonic()+180
                previous=0
                while time.monotonic()<deadline and vm.poll() is None:
                    data=serial.read_bytes() if serial.exists() else b""
                    if len(data)>previous:
                        previous=len(data)
                        deadline=time.monotonic()+180
                    if complete in data: return
                    if any(s in data for s in (b"Panique",b"Erreur fatale",b"panicked",b"Exception noyau")):
                        raise RuntimeError(data[-2000:].decode(errors="replace"))
                    time.sleep(0.25)
                raise RuntimeError("Banc sans progression")
            name=f"ethernet-phase{phase}-{arch}"
            try:
                demarrage.boot(name,observe)
            finally:
                serial=NOVA/"build"/f"serie-{name}.log"
                if serial.exists(): shutil.copyfile(serial,run/"serie.log")
                manifest={p.name:sha(p) for p in run.iterdir() if p.is_file()}
                (run/"sha256.json").write_text(json.dumps(manifest,indent=2)+"\n")
    finally:
        if marker.exists() and json.loads(marker.read_text())==owner:
            marker.unlink()
            lock.rmdir()

if __name__=="__main__": main()
