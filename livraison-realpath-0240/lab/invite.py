"""Session réelle nsh. Sortie série de présentation, compteurs du disque via QMP."""
import hashlib
import json
import os
from contextlib import ExitStack
from pathlib import Path
import socket
import subprocess
import sys
import time
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "novaOS/tests"))
from cli import Terminal
from bloc import qmp
from journalisation import arret
from pipe_natif import fixture

def sha(path):
    with path.open("rb") as src:
        return hashlib.file_digest(src, "sha256").hexdigest()

def main(arch, sector, dest):
    dest.mkdir(parents=True, exist_ok=False)
    image, _, _ = fixture(dest, sector, {"temoin": b"P9\n"})
    serial = dest / "nsh.serial"
    records = []
    with ExitStack() as stack:
        client, peer = socket.socketpair()
        stack.enter_context(client)
        stack.enter_context(peer)
        client.settimeout(20)
        channel = stack.enter_context(client.makefile("rwb", buffering=0))
        monitor = SimpleNamespace(stdin=channel, stdout=channel)
        vm = stack.enter_context(Terminal(arch, sector, image, "realpath", journal=serial, qmp_bloc=peer.fileno()))
        peer.close()
        qmp(monitor, "qmp_capabilities")
        vm.until(b"nsh> ")
        def snapshot():
            stats, = [r["stats"] for r in qmp(monitor,"query-blockstats") if r["device"] == "d"]
            return [sha(image),stats["wr_operations"],stats["wr_bytes"],stats["flush_operations"]]
        def run(cmd, status, required=(), manual=False):
            vm.vm.stdin.write(f"echo DDEBUT; {cmd}; echo DFIN=$?\n".encode())
            vm.until(b"\r\nDDEBUT\r\n")
            raw = b""
            if manual:
                raw = vm.until(b"man realpath 1/")
                vm.vm.stdin.write(b"q")
            raw += vm.until(f"DFIN={status}\r\n".encode(),timeout=12)
            vm.until(b"nsh> ")
            for needle in required:
                assert needle.encode() in raw, (cmd,needle,raw)
            records.append(dict(cmd=cmd,status=status,serie_hex=raw.hex()))
        for _ in range(15):
            before = snapshot()
            time.sleep(2)
            if before == snapshot():
                break
        else:
            raise RuntimeError("P9 ne se stabilise pas")
        for tag, title, heading in [("fr-CA","Afficher les chemins","Capacités"),("fr--CA","Print resolved","Capabilities")]:
            run("export LANG="+tag,0)
            run("realpath --help",0,[title,"--zero"])
            run("realpath -zz --he --bad",0,[title])
            run("realpath --version",0,["realpath (nova-realpath) 0.24.0"])
            run("man realpath",0,[title,heading],manual=True)
        run("realpath -x",1,["invalid option"])
        after = snapshot()
        assert before == after, (before,after)
        run("realpath /objet/absent",127)
        run("realpath -z a/b",127)
        run("nsh -c 'realpath a/b'",127)
        run("nsh -c 'realpath / | wc -l'",3)
        arret(vm,2)
    kernel = Path(os.environ["NOVA_KERNEL_ELF"])
    boot = dest/"noyau-demarre.elf"
    iso = serial.with_suffix(".iso")
    subprocess.run(["xorriso","-osirrox","on","-indev",iso,"-extract","/boot/novaos.elf",boot],check=True)
    assert sha(boot) == sha(kernel)
    (dest/"resultats.json").write_text(json.dumps(dict(arch=arch,secteur=sector,profil="nsh P9, QEMU 2 vCPU 256 Mio",
        iso=sha(iso),noyau=sha(kernel),avant=before,apres=after,commandes=records),ensure_ascii=False,indent=1)+"\n")

if __name__ == "__main__":
    main(sys.argv[1],int(sys.argv[2]),Path(sys.argv[3]).resolve())
