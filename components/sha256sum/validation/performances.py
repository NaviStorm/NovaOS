#!/usr/bin/env python3
"""Chronométrer le processus sha256sum sur Darwin. Les grands corpus sont explicites."""
import argparse, hashlib, json, os, platform, shutil, statistics, subprocess, tempfile, time
from pathlib import Path

MiB=1<<20
GiB=1<<30
p=argparse.ArgumentParser(description=__doc__)
p.add_argument("binary",type=Path)
p.add_argument("--large",action="store_true",help="fichier et flux de 4 Gio + 4097, puis un bit changé")
p.add_argument("--above-ram",action="store_true",help="fichier et flux de RAM + 4097 octets")
args=p.parse_args()
binary=args.binary.resolve(strict=True)
ram=int(subprocess.check_output(["sysctl","-n","hw.memsize"]))
def sysctl(k):return subprocess.check_output(["sysctl","-n",k],text=True).strip()
def marks(size):
    for off in sorted(set([*range(0,size,GiB),size-64])):
        yield off,hashlib.sha512(f"ARC027-{off}".encode()).digest()[:size-off]
def blocks(size):
    pos=0
    zeros=bytes(MiB)
    for off,marker in marks(size):
        while pos<off:
            n=min(off-pos,MiB)
            yield memoryview(zeros)[:n]
            pos+=n
        yield marker
        pos=off+len(marker)
    while pos<size:
        n=min(size-pos,MiB)
        yield memoryview(zeros)[:n]
        pos+=n

def measure(cmd,expected,size,kind,stream=None):
    with tempfile.TemporaryFile(dir=tmp) as output:
        start=time.monotonic()
        with subprocess.Popen([binary,*cmd],stdin=subprocess.PIPE if stream else subprocess.DEVNULL,stdout=output) as child:
            if stream:
                total=0
                for chunk in stream:
                    child.stdin.write(chunk);total+=len(chunk)
                child.stdin.close()
                assert total==size
            _,status,res=os.wait4(child.pid,0)
            child.returncode=os.waitstatus_to_exitcode(status)
            assert child.returncode==0
        elapsed=time.monotonic()-start
        output.seek(0)
        assert output.read()==expected
        return dict(profil=kind,octets=size,temps_s=elapsed,cpu_s=res.ru_utime+res.ru_stime,
                    pic_rss_octets=res.ru_maxrss,mio_s=size/elapsed/MiB)

report=dict(binaire=str(binary),sha256_binaire=hashlib.sha256(binary.read_bytes()).hexdigest(),
    arch=platform.machine(),systeme=platform.platform(),cpu=sysctl("machdep.cpu.brand_string"),
    cpus=int(sysctl("hw.logicalcpu")),ram=ram,isa_sha256=sysctl("hw.optional.arm.FEAT_SHA256"),
    cache="cache macOS libre, aucune purge",mesures=[],synthese=[])
def record(kind,rows):
    speeds=[row["mio_s"] for row in rows]
    report["mesures"]+=rows
    report["synthese"].append(dict(profil=kind,octets=rows[0]["octets"],n=len(rows),
        mediane_mio_s=statistics.median(speeds),min_mio_s=min(speeds),max_mio_s=max(speeds)))

with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as tmp:
    os.chdir(tmp)
    sizes=[MiB,16*MiB,64*MiB]
    if args.large or args.above_ram:
        # Un tri externe actif peut occuper la RAM et une grande partie du disque.
        processes=subprocess.check_output(["ps","-axo","comm="],text=True).splitlines()
        if any(Path(v.strip()).name in {"gsort","sort"} for v in processes):
            raise SystemExit("Tri actif, différer les mesures lourdes.")
        sizes=[]
        if args.large:sizes.append(4*GiB+4097)
        if args.above_ram:sizes.append(ram+4097)
    for size in sizes:
        free = shutil.disk_usage(tmp).free
        reserve = 8 * GiB
        required = len(list(marks(size))) * MiB
        if free < reserve + required:
            raise SystemExit("Réserve disque insuffisante pour le corpus creux.")
        if free < reserve + size:
            # Tester le comportement réel dans le répertoire du corpus avant le grand truncate.
            with tempfile.TemporaryFile(dir=tmp) as probe:
                probe.seek(8 * MiB)
                probe.write(b"1")
                probe.flush()
                if os.fstat(probe.fileno()).st_blocks * 512 >= MiB:
                    raise SystemExit("Ce volume alloue les zones creuses et son espace libre est insuffisant.")
        file=Path("corpus")
        with file.open("wb") as f:
            f.truncate(size)
            for off,marker in marks(size):
                f.seek(off);f.write(marker)
        info=file.stat()
        assert info.st_size==size
        digest=hashlib.sha256()
        for part in blocks(size):digest.update(part)
        expected=digest.hexdigest().encode()
        repeats=1 if size>GiB else 3
        for kind,cmd,name in [("fichier",["corpus"],b"corpus"),("flux",[],b"-")]:
            rows=[measure(cmd,expected+b"  "+name+b"\n",size,kind,blocks(size) if kind=="flux" else None) for _ in range(repeats)]
            for row in rows:row.update(sha256=expected.decode(),allocation_fichier=info.st_blocks*512)
            record(kind,rows)
        if 4*GiB<size<5*GiB:
            with file.open("r+b") as f:
                f.seek(-1,2);last=f.read(1);f.seek(-1,2);f.write(bytes([last[0]^1]))
            with file.open("rb") as f:changed=hashlib.file_digest(f,"sha256").hexdigest()
            assert changed!=expected.decode()
            row=measure(["corpus"],changed.encode()+b"  corpus\n",size,"fichier-un-bit-modifie")
            row.update(sha256=changed);record(row["profil"],[row])
    if not args.above_ram and not args.large:
        lines=[]
        for i in range(1000):
            name=f"petit-{i}"
            data=hashlib.shake_256(name.encode()).digest(1024)
            Path(name).write_bytes(data);lines.append(f"{hashlib.sha256(data).hexdigest()}  {name}\n")
        Path("SUMS").write_text("".join(lines))
        record("verification-1000",[measure(["-c","--quiet","SUMS"],b"",1000*1024+Path("SUMS").stat().st_size,"verification-1000") for _ in range(3)])
print(json.dumps(report,ensure_ascii=False,indent=2))
