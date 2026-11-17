"""Témoin de transport opaque et de conservation de P9 sur une copie vierge d’écritures."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import threading

from adaptateur import servir, lire
SERVICE = Path(__file__).resolve().parents[1] / "service"
sys.path.insert(0, str(SERVICE))
from paquet_qemu import Serial
from shell_qemu import command


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def essai(arch, sector, iso, source, out):
    out.mkdir(exist_ok=False, parents=True)
    disk = out / "copie.p9"
    shutil.copyfile(source, disk)
    before = digest(disk)
    # sockaddr_un est limité à 104 octets sur Darwin. Le nom relatif s’ouvre
    # dans le cwd privé du QEMU, sans raccourci sous /tmp.
    errors = []
    echo = []
    def listen(listener, left):
        try:
            with disk.open("r+b",buffering=0) as f, (out / "nbd.log").open("x") as trace:
                def observe(cmd, offset, n, flags, err):
                    print(cmd, offset, n, flags, err, file=trace, flush=True)
                servir(f.fileno(),listener,left,sector,observe)
        except Exception as exc:
            errors.append(repr(exc))

    def peer(right):
        try:
            with right:
                right.settimeout(60)
                echo.append(bytes(lire(right,8)).hex())
                if echo != [b"aller\0\x09\xff".hex()]:
                    raise ValueError("charge aller différente")
                right.sendall(b"retour\xff\x09\0")
                while right.recv(128):
                    raise ValueError("octets supplémentaires")
        except Exception as exc:
            errors.append(repr(exc))

    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as temp:
        cwd = Path.cwd()
        with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as listener:
            os.chdir(temp)
            try:
                listener.bind("nbd.sock")
                listener.listen(1)
            finally:
                os.chdir(cwd)
            listener.settimeout(60)
            left,right = socket.socketpair()
            workers = [threading.Thread(target=listen,args=(listener,left)),threading.Thread(target=peer,args=(right,))]
            for t in workers:t.start()
            args = command(arch,sector,disk,controls=["-serial","stdio","-display","none","-monitor","none"])
            args.extend(["-trace", "enable=virtio_blk_handle_*,file=" + str((out / "virtio.trace").resolve())])
            i=args.index("-drive")+1
            args[i]="if=none,id=d,format=raw,file.driver=nbd,file.server.type=unix,file.server.path=nbd.sock,cache=none,werror=report,rerror=report"
            env=dict(os.environ,NOVA_ARCH=arch,NOVA_ISO=str(iso.resolve()),NOVA_VCPUS="2",NOVA_ARM_CPU="max")
            raw=out/"invite.serial"
            ok=False
            with raw.open("xb") as log, subprocess.Popen(args,cwd=temp,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0) as vm:
                try:
                    match = Serial(vm,log).expect(rb"NAUTH transport (OK|ECHEC)",60)
                    ok = match.group(1) == b"OK"
                except (EOFError,TimeoutError) as exc:
                    errors.append(repr(exc))
                finally:
                    if vm.poll() is None:vm.terminate()
                    vm.wait(timeout=10)
                    log.write(vm.stdout.read())
            for t in workers:t.join()
    result=dict(architecture=arch,secteur=sector,iso=digest(iso),p9_avant=before,p9_apres=digest(disk),
                serial=digest(raw),aller=echo,retour="7265746f7572ff0900",erreurs=errors,commande=args)
    result["succes"]=ok and not errors and result["p9_apres"]==before
    (out/"resultat.json").write_text(json.dumps(result,indent=2)+"\n")
    print(arch,sector,result["succes"],errors)
    return result["succes"]

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("sector",type=int,choices=[512,4096])
    for name in ("iso","source","out"):p.add_argument(name,type=Path)
    a=p.parse_args()
    raise SystemExit(0 if essai(a.arch,a.sector,a.iso,a.source,a.out) else 1)
