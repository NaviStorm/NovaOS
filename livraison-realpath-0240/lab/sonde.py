"""Sonde SDK dans un invité réel, flux en mémoire et P9 en lecture seule."""
import fcntl
import json
import os
from pathlib import Path
import re
import select
import statistics
import struct
import subprocess
import sys
import time
from invite import ROOT, fixture, sha
from bloc_qemu import command

def vectors(out):
    from options import observe, GNU, NOVA, fixtures
    import tempfile
    import itertools
    records=[]
    ids=[]
    def num(n):return struct.pack("<I",n)
    def blob(data):return num(len(data))+data
    cwd=Path.cwd()
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as tmp:
        os.chdir(tmp)
        try:
            fixtures()
            names=[".", "tree/file", "alias/..", "absolute/..", "broken", "cycle", "missing/x", "secret/child", "", "tree/file/.."]
            modes=["-E","-e","-m","--canonicalize","--canonicalize-existing","--canonicalize-missing"]
            links=["-P","-s","-L","--physical","--strip","--logical","--no-symlinks"]
            cases=[[mode,link,"--relative-to=.", "--relative-base=.", "-z", *names]
                   for mode,link in itertools.product(modes,links)]
            cases.extend([
                ["--relative-to=.",*names],["-q","--relative-to=.",*names],
                ["--quiet","--zero","--relative-to",".","--",*names],
                ["-mLs","--relative-to=tree","--relative-base=tree","tree/leaf","alias/.."],
                ["-sm","--relative-to=tree","--relative-base=tree/leaf","tree/file"],
                ["-eE","-sLP","--relative-base=tree","tree/file"],
                ["--relative-base=", "."], ["-qe","--relative-to=tree/file","."],
                [],["--"],["-x"],["--rel"],["--relative-to"],["--zero=x"],["--bad"]
            ])
            for args in cases:
                raw=observe(GNU,args,{"LC_ALL":"C"})
                assert raw==observe(NOVA,args,{"LC_ALL":"C"})
                vals=list(map(os.fsencode,args))
                # La racine synthétique représente le cwd de la fixture GNU.
                prefix=os.fsencode(Path.cwd())
                stdout=bytes.fromhex(raw["out"]).replace(prefix+b"/",b"/").replace(prefix+b"\n",b"/\n")
                records.append(num(len(vals))+b"".join(map(blob,vals))+num(raw["statut"])+blob(stdout)+blob(bytes.fromhex(raw["err"])))
                ids.append(args)
        finally:
            Path("secret").chmod(0o700)
            os.chdir(cwd)
    out.write_bytes(num(len(ids))+b"".join(records))
    return ids

def main(arch,sector,out):
    out.mkdir(parents=True,exist_ok=False)
    ids=vectors(out/"vecteurs.bin")
    target="aarch64-unknown-none-softfloat" if arch=="aarch64" else "x86_64-unknown-none"
    tree=Path(os.environ["TMPDIR"])/("sdk-"+arch)
    env=dict(os.environ,NOVA_ARCH=arch,CARGO_TARGET_DIR=str(tree),NOVA_RAM="256",NOVA_VCPUS="2",
        RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    cargo=["rustup","run","stable","cargo","build","--release","--locked","--offline","--target",target]
    subprocess.run([*cargo,"--bin","sonde","--features","qualification"],cwd=ROOT/"components/realpath",
        env={**env,"REALPATH_VECTEURS":str(out/"vecteurs.bin"),"RUSTFLAGS":f"-Crelocation-model=static -Cno-redzone=yes -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T{ROOT}/novaOS/user/novafs/service.ld"},check=True)
    elf=tree/target/"release/sonde"
    env.update(NOVA_THREAD_PROBE_ELF=str(elf),NOVA_ISO=str(out/"sdk.iso"))
    subprocess.run([*cargo,"--features","thread-probe"],cwd=ROOT/"novaOS",env=env,check=True)
    kernel=tree/target/"release/novaos"
    env["NOVA_KERNEL_ELF"]=str(kernel)
    assert elf.read_bytes() in kernel.read_bytes()
    (out/"sonde.elf").write_bytes(elf.read_bytes())
    subprocess.run(["sh",ROOT/"novaOS/scripts/iso.sh","sdk-example",env["NOVA_ISO"]],env=env,check=True)
    boot=out/"noyau-demarre.elf"
    subprocess.run(["xorriso","-osirrox","on","-indev",env["NOVA_ISO"],"-extract","/boot/novaos.elf",boot],check=True)
    assert sha(boot)==sha(kernel)
    disk,_,_=fixture(out,sector,{"temoin":b"P9\n"})
    before=sha(disk)
    cmd=command(arch,sector,disk,readonly=True,controls=["-display","none","-monitor","none","-serial","stdio"])
    records=[]
    starts={}
    ok=False
    with (out/"sdk.serial").open("wb",buffering=0) as log,subprocess.Popen(cmd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0) as vm:
        limit=time.monotonic()+150
        buf=b""
        try:
            while time.monotonic()<limit:
                if not select.select([vm.stdout],[],[],0.5)[0]:
                    continue
                block=os.read(vm.stdout.fileno(),65536)
                stamp=time.perf_counter_ns()
                if not block:
                    break
                log.write(block)
                lines=(buf+block).split(b"\n")
                buf=lines.pop()
                for line in lines:
                    if f"REALPATH MATRICE {len(ids)} OK".encode() in line:
                        ok=True
                    m=re.search(rb"REALPATH DEBUT (\d+) (\d+) (\d+) (\d+)",line)
                    if m:
                        n,size,reps,i=map(int,m.groups())
                        starts[n,i]=(stamp,size,reps)
                    m=re.search(rb"REALPATH FIN (\d+) (\d+) (\d+)",line)
                    if m:
                        n,i,heap=map(int,m.groups())
                        start,size,reps=starts[n,i]
                        starts[n,i+1]=(stamp,size,reps)
                        records.append(dict(n=n,octets_par_nom=size,passage=i,repetitions=reps,tas_octets=heap,ns=stamp-start))
                    if any(marker in line for marker in (b"ASSERT [",b"REALPATH ECART",b"Panique du noyau",b"panicked at")):
                        raise AssertionError(line)
                if b"SDK exemple admis sortie=0 restitution=OK" in b"\n".join(lines):
                    break
            else:
                raise TimeoutError(str(out))
        finally:
            vm.kill()
            vm.wait()
    assert ok and len(records)==12 and before==sha(disk)
    assert b"SDK exemple admis sortie=0 restitution=OK" in (out/"sdk.serial").read_bytes()
    medians={}
    for n in (1,32,4):
        ns=statistics.median(r["ns"]/r["repetitions"] for r in records if r["n"]==n and r["passage"]>0)
        medians[n]=dict(ns_par_appel=ns,chemins_s=n*1e9/ns)
    (out/"resultats.json").write_text(json.dumps(dict(architecture=arch,secteur=sector,cas=ids,mesures=records,
        medianes=medians,disque=before,iso=sha(out/"sdk.iso"),noyau=sha(kernel),sonde=sha(elf),vecteurs=sha(out/"vecteurs.bin"),
        commande=cmd),ensure_ascii=False,indent=1)+"\n")

if __name__=="__main__":
    with (Path(os.environ["NOVAOS_TMPDIR"])/"arc027-mesures.lock").open("a") as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        main(sys.argv[1],int(sys.argv[2]),Path(sys.argv[3]).resolve())
