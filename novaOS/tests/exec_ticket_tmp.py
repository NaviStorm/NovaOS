"""Concurrence entre threads, reçu de lancement et restitution physique du tmp sur P9."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import time
import select
import subprocess
import tempfile
from tmp_prive import ROOT, command, fixture_p9

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preuves",type=Path,required=True)
    parser.add_argument("--secteur",type=int,choices=(512,4096),required=True)
    args=parser.parse_args()
    dest=args.preuves.resolve()
    dest.mkdir(parents=True,exist_ok=False)
    arch=os.environ["NOVA_ARCH"]
    sector=args.secteur
    target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[arch]
    kernel=ROOT/"target"/target/"release/novaos"
    fs=ROOT.parent/"novaFS"
    host="aarch64-apple-darwin"
    subprocess.run([fs/"lab/run","build","--offline","--lib","--bin","novafs"],
                   env={**os.environ,"NOVAFS_TARGET":host},check=True)
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as temp:
        work=Path(temp)
        lib=fs/"target"/host/"release"
        oracle=work/"oracle"
        subprocess.run(["rustup","run","stable","rustc","--edition=2024","--target",host,
                        "-L","dependency="+str(lib/"deps"),"--extern","novafs="+str(lib/"libnovafs.rlib"),
                        Path(__file__).with_name("tmp_prive_oracle.rs"),"-o",oracle],check=True)
        iso=work/"tmp-contention.iso"
        subprocess.run(["sh",ROOT/"scripts/iso.sh","tmp-contention",iso],check=True)
        image=work/"volume.raw"
        fixture_p9(image,sector)
        catalogue=[oracle,image,str(sector),"catalogue"]
        public=subprocess.check_output(catalogue)
        (dest/"catalogue-avant.txt").write_bytes(public)
        archive(image,dest/"volume-avant.raw.gz")
        archive(iso,dest/"demarrage.iso.gz")
        archive(kernel,dest/"noyau.elf.gz")
        commandline=command(arch,sector,image,controls=["-display","none","-monitor","none","-serial","stdio"])
        env={**os.environ,"NOVA_ISO":str(iso),"NOVA_VCPUS":"2"}
        log=dest/"invite.serial"
        markers=[b"TMP snapshot sous reservation",b"TMP snapshot apres drainage"]
        snapshots=[work/"sous-reservation.raw",work/"apres-drainage.raw"]
        with log.open("xb") as stream, subprocess.Popen(commandline,env=env,stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0) as vm:
            data=bytearray()
            try:
                stage=0
                deadline=time.monotonic()+180
                while time.monotonic()<deadline:
                    if not select.select([vm.stdout],[],[],1)[0]: continue
                    chunk=os.read(vm.stdout.fileno(),16384)
                    if not chunk: break
                    stream.write(chunk); stream.flush(); data.extend(chunk)
                    if stage<2 and markers[stage] in data:
                        # Le client bloque la progression avant de relâcher le WRITE
                        # ou le témoin survivant. Aucun autre écrivain n'est actif.
                        shutil.copyfile(image,snapshots[stage])
                        archive(snapshots[stage],dest/(snapshots[stage].name+".gz"))
                        vm.stdin.write(b"!")
                        stage+=1
                    if re.search(rb"SMP arret motif=[^\n]+\n",data): break
            finally:
                if vm.poll() is None: vm.kill()
                vm.wait()
                archive(image,dest/"volume-apres.raw.gz")
        serial=log.read_bytes()
        race=re.search(rb"EXEC ticket pid=(\d+) tid0=(\d+) status0=(\d+) tid1=(\d+) status1=(\d+) enfant=(\d+)",serial)
        assert race and race[2]!=race[4] and sorted([int(race[3]),int(race[5])])==[0,12], log
        assert b"anciennes formes refusees recu relu collecte unique revocation expiration OK" in serial, log
        blocks=re.search(rb"TMP contention blocs_avant=(\d+) disponibles_reservation=(\d+) apres_drainage=(\d+) ancien_handle_refuse autre_contenu_verifie",serial)
        assert blocks and int(blocks[2])<int(blocks[1]) and int(blocks[3])>int(blocks[1]), log
        assert b"TMP retrait physique differe ecrivain P9" in serial and b"BLOC session terminee ok=true" in serial,log
        assert b"SMP arret motif=normal" in serial and b"Panique du noyau" not in serial,log
        final=subprocess.check_output(catalogue)
        (dest/"catalogue-apres.txt").write_bytes(final)
        assert final==public
        subprocess.run([oracle,image,str(sector),"propre"],check=True)
        extents=work/"oracle-extents"
        subprocess.run(["rustup","run","stable","rustc","--edition=2024","--target",host,
                        "-L","dependency="+str(lib/"deps"),"--extern","novafs="+str(lib/"libnovafs.rlib"),
                        Path(__file__).with_name("tmp_extents_oracle.rs"),"-o",extents],check=True)
        with (dest/"blocs-physiques.txt").open("xb") as evidence:
            subprocess.run([extents,str(sector),*snapshots,image],stdout=evidence,check=True)
        record={"architecture":arch,"secteur":sector,"vcpus":2,
                "qemu":subprocess.check_output([f"qemu-system-{arch}","--version"],text=True).splitlines()[0],
                "course":list(map(int,race.groups())),
                "blocs":list(map(int,blocks.groups()))}
        (dest/"resultats.json").write_text(json.dumps(record,indent=2)+"\n")
        print(record)

def archive(source,destination):
    with source.open("rb") as a, gzip.open(destination,"wb",compresslevel=1) as b:
        shutil.copyfileobj(a,b)
    with gzip.open(destination,"rb") as a, source.open("rb") as b:
        assert hashlib.file_digest(a,"sha256").digest()==hashlib.file_digest(b,"sha256").digest()

if __name__=="__main__":
    main()
