"""Témoin de transport opaque et de conservation de P9 sur une copie vierge d’écritures."""
import argparse
import hashlib
import json
import os
import queue
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import threading

from adaptateur import lire, servir
from qualification_tls import qualify
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
    tls = {}
    pressure = queue.Queue(maxsize=1)
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
                tls.update(qualify(right))
                right.sendall(b"NA24-pression")
                count = pressure.get(timeout=60)
                if not 1 <= count <= 1024:
                    raise ValueError("compteur de saturation")
                for index in range(1, count+1):
                    if lire(right,448) != bytes([index % 256])*448:
                        raise ValueError("perte ou duplication au drainage")
                tls["cellules_drainees"] = count
                right.shutdown(socket.SHUT_RDWR)
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
            left.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 4096)
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
                    terminal = Serial(vm,log)
                    saturated = terminal.expect(rb"NAUTH (?:saturation ([0-9]+)\r?\n|transport ECHEC)",60)
                    if saturated.group(1) is None:
                        raise ValueError("la sonde a refusé la recette avant saturation")
                    pressure.put(int(saturated.group(1)))
                    match = terminal.expect(rb"NAUTH transport (OK|ECHEC)",60)
                    ok = match.group(1) == b"OK"
                except (EOFError,TimeoutError,ValueError) as exc:
                    errors.append(repr(exc))
                finally:
                    if vm.poll() is None:vm.terminate()
                    vm.wait(timeout=10)
                    log.write(vm.stdout.read())
            for t in workers:t.join()
    result=dict(architecture=arch,secteur=sector,iso=digest(iso),p9_avant=before,p9_apres=digest(disk),
                serial=digest(raw), admissibilite_tls=tls, adaptateur=digest(Path(os.environ["NOVA_ADAPTATEUR"])), erreurs=errors,commande=args)
    result["succes"]=ok and bool(tls) and not errors and result["p9_apres"]==before
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
