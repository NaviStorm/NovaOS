"""Sonde utilisateur du moteur et de l'analyseur CUT via le profil SDK existant."""
import argparse
import gzip
import json
import os
import re
import select
import tarfile
import struct
import subprocess
import sys
import time
from pathlib import Path
from invite import ROOT, digest, fixture
from bloc_qemu import command

def prepare(archive,out):
    data=bytearray();ids=[]
    def number(n):data.extend(struct.pack("<I",n))
    def blob(b):number(len(b));data.extend(b)
    for i,line in enumerate(gzip.open(archive),1):
        row=json.loads(line)
        if not row.get("GNU") or not row["ok"]:continue
        args=list(map(os.fsencode,row["args"]))
        # Les opérations portant sur de vrais fichiers relèvent de la recette nsh.
        named={b"absent",b"repertoire",b"a",b"b",b"-fichier"}
        if named.intersection(args) or any(b"absent" in v for v in args):continue
        ref=row["GNU"];src=bytes.fromhex(row["stdin"])
        expected=bytes.fromhex(ref["stdout"])
        if ref["rc"]:
            error=bytes.fromhex(ref["stderr"]).split(b": ",1)[1]
            expected=error.rsplit(b"\nTry ",1)[0]
        ids.append(i);number(i);number(row["locale"]!="C");number(len(args))
        for arg in args:blob(arg)
        n=len(src)-len(src.lstrip(b"x"))
        if n<32:n=0
        number(n);blob(src[n:]);number(ref["rc"]);blob(expected)
    (out/"vecteurs.bin").write_bytes(data);return ids

def query(vm,op):
    vm.stdin.write((json.dumps({"execute":op})+"\n").encode());vm.stdin.flush()
    while True:
        if not select.select([vm.stdout],[],[],10)[0]:raise TimeoutError(op)
        msg=json.loads(vm.stdout.readline())
        if "return" in msg:return msg["return"]
        if "error" in msg:raise RuntimeError(msg)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"]);p.add_argument("sector",type=int,choices=[512,4096])
    p.add_argument("out",type=Path);p.add_argument("--large",action="store_true",help="272 Mio en projection TSV, 16 Mio pour les deux autres traitements, sur 256 Mio invités")
    p.add_argument("--prevol", action="store_true", help="Démarrer la sonde exacte du prévol archivé du candidat")
    a=p.parse_args()
    if a.prevol and not a.large:
        p.error("La sonde du prévol a été construite pour --large")
    out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    base=ROOT/"livraison-cut-0240";tmp=Path(os.environ["NOVAOS_TMPDIR"])
    ids=prepare(base/"preuves/options.jsonl.gz",out)
    arch=a.arch;target="aarch64-unknown-none-softfloat" if arch=="aarch64" else "x86_64-unknown-none"
    tree=tmp/f"cut-sonde-{arch}"
    env=dict(os.environ,CARGO_TARGET_DIR=str(tree),CUT_VECTORS=str(out/"vecteurs.bin"),CUT_LARGE=str(int(a.large)),
             RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    flags=f"-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T{ROOT}/novaOS/user/novafs/service.ld"
    cargo=["rustup","run","stable","cargo","build","--release","--locked","--offline","--target",target]
    if a.prevol:
        prevol = base / "preuves/creneau"
        manifest = json.loads((prevol / "prevol-qemu.json").read_text())
        assert len(ids) == manifest["vecteurs"] and digest(out/"vecteurs.bin") == manifest["sha256_vecteurs"]
        with tarfile.open(prevol / "prevol-elfs.tar.gz") as archive:
            for name in ("cut", "sonde"):
                path = out / f"{name}.elf"
                with archive.extractfile(f"{arch}/{name}.elf") as src:
                    path.write_bytes(src.read())
                assert digest(path) == manifest["binaires"][arch][name]["sha256"]
        elf = out / "sonde.elf"
    else:
        subprocess.run([*cargo,"--features","qualification","--bin","sonde"],cwd=ROOT/"components/cut",env={**env,"RUSTFLAGS":flags},check=True)
        elf=tree/target/"release/sonde"
    env.update(NOVA_THREAD_PROBE_ELF=str(elf),NOVA_ARCH=arch,NOVA_RAM="256",NOVA_VCPUS="2",NOVA_ISO=str(out/"sonde.iso"))
    subprocess.run([*cargo,"--features","thread-probe"],cwd=ROOT/"novaOS",env=env,check=True)
    kernel=tree/target/"release/novaos";env["NOVA_KERNEL_ELF"]=str(kernel)
    assert elf.read_bytes() in kernel.read_bytes()
    subprocess.run(["sh",ROOT/"novaOS/scripts/iso.sh","sdk-example",env["NOVA_ISO"]],env=env,check=True,stdout=subprocess.DEVNULL)
    extracted=out/"demarre.elf"
    subprocess.run(["xorriso","-osirrox","on","-indev",env["NOVA_ISO"],"-extract","/boot/novaos.elf",extracted],check=True)
    assert digest(extracted)==digest(kernel)
    image,_,_=fixture(out,a.sector,{"temoin":b"p9"})
    initial_disk = digest(image)
    log=out/"sonde.serial";params=command(arch,a.sector,image,readonly=True,controls=["-display","none","-monitor","none","-qmp","stdio","-serial",f"file:{log}"])
    with subprocess.Popen(params,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,bufsize=0) as vm:
        starts={};samples=[];matrix=0
        try:
            query(vm,"qmp_capabilities");ram=query(vm,"query-memory-size-summary")["base-memory"]
            deadline=time.monotonic()+420;offset=0;partial=b"";done=False
            while not done and time.monotonic()<deadline and vm.poll() is None:
                if log.exists():
                    with log.open("rb") as f:f.seek(offset);new=f.read();offset=f.tell()
                    lines=(partial+new).split(b"\n");partial=lines.pop()
                    for line in lines:
                        stamp=time.perf_counter()
                        m=re.search(rb"CUT DEBUT (\d+) (\d+) (\d+)",line)
                        if m:
                            test,rep,n=map(int,m.groups());starts[test,rep]=(stamp,n)
                        m=re.search(rb"CUT FIN (\d+) (\d+)",line)
                        if m:
                            test,rep=map(int,m.groups());begin,n=starts[test,rep]
                            starts[test,rep+1]=(stamp,n)
                            samples.append(dict(test=test,passage=rep,octets=n,secondes=stamp-begin))
                        m=re.search(rb"CUT MATRICE (\d+) OK",line)
                        if m:matrix=int(m[1])
                        if b"SDK exemple admis sortie=0 restitution=OK" in line:done=True
                        if b"Panique du noyau" in line or b"ASSERT [" in line:raise RuntimeError(log.read_text(errors="replace")[-1300:])
                time.sleep(.004)
            assert done and len(samples)==12 and matrix==len(ids),(samples,matrix)
        finally:vm.kill();vm.wait()
    assert digest(image) == initial_disk
    assert {(m["test"], m["passage"]) for m in samples} == {(i, j) for i in range(3) for j in range(4)}
    if a.large:
        assert min(m["octets"] for m in samples if m["test"] == 0) > ram
    summary=dict(arch=arch,secteur=a.sector,ram_octets_qmp=ram,profil="SDK mémoire, P9 attaché en lecture seule",
                 cas_reussis=ids,mesures=samples,commande=params,iso_sha256=digest(out/"sonde.iso"),
                 noyau_sha256=digest(kernel),elf_sonde_sha256=digest(elf),oracle_sha256=digest(out/"vecteurs.bin"))
    if a.prevol:
        summary.update(elf_cut_sha256=digest(out/"cut.elf"), champs_longs_reussis=3,
                       volume_avant_sha256=initial_disk, volume_apres_sha256=digest(image))
    (out/"resultats.json").write_text(json.dumps(summary,ensure_ascii=False,indent=1)+"\n")
    print(arch,a.sector,"cas",matrix,"RAM",ram,"flux",samples[0]["octets"])
if __name__=="__main__":main()
