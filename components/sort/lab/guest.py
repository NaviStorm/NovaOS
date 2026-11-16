"""Qualification du cœur avec le lanceur SDK et contrôle interactif sur quatre disques P9."""
from contextlib import ExitStack
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
from types import SimpleNamespace

ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from threads import exercise_example
from cli import Terminal
from session_cli import fixture_p9
from journalisation import sortie
from bloc import qmp

def main():
    p=argparse.ArgumentParser()
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("out",type=Path)
    args=p.parse_args()
    arch=args.arch
    out=args.out.resolve()
    out.mkdir(parents=True,exist_ok=False)
    root=ROOT/"components/sort"
    target="x86_64-unknown-none" if arch=="x86_64" else "aarch64-unknown-none-softfloat"
    build=Path(os.environ["NOVAOS_TMPDIR"])/("arc027-sdk-sort-"+arch)
    os.environ.update(NOVA_ARCH=arch,NOVA_VCPUS="2",NOVA_RAM="256",CARGO_TARGET_DIR=str(build),
                      RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    elf=Path(subprocess.check_output([root/"build.sh",arch,"--features","qualification"],text=True).splitlines()[-1])
    os.environ["NOVA_THREAD_PROBE_ELF"]=str(elf)
    subprocess.run(["rustup","run","stable","cargo","build","-p","novaos","--target",target,"--release","--locked","--offline","--features","thread-probe"],cwd=ROOT/"novaOS",check=True)
    os.environ["NOVA_KERNEL_ELF"]=str(build/target/"release/novaos")
    sdk=exercise_example(arch,2)
    sdk["witness_sha256"]=hashlib.sha256(elf.read_bytes()).hexdigest()
    (out/"sdk.json").write_text(json.dumps(sdk,indent=2)+"\n")
    shutil.copy2(ROOT/"novaOS/build"/f"serie-{arch}-sdk-example-2cpu.log",out/"sdk.serial")
    del os.environ["NOVA_KERNEL_ELF"]
    subprocess.run(["make","-C",ROOT/"novaOS","ARCH="+arch,"kernel"],check=True,env={k:v for k,v in os.environ.items() if k!="CARGO_TARGET_DIR"})
    for sector in [512,4096]:
        folder=out/str(sector)
        folder.mkdir()
        disk=folder/"volume.raw"
        fixture_p9(disk,sector)
        with ExitStack() as stack:
            a,b=socket.socketpair()
            stack.enter_context(a);stack.enter_context(b)
            ch=stack.enter_context(a.makefile("rwb",buffering=0))
            mon=SimpleNamespace(stdin=ch,stdout=ch)
            guest=stack.enter_context(Terminal(arch,sector,disk,"sort",journal=folder/"uart.serial",qmp_bloc=b.fileno()))
            b.close()
            qmp(mon,"qmp_capabilities")
            ram=qmp(mon,"query-memory-size-summary")["base-memory"]
            guest.until(b"nsh> ")
            results={}
            for lang in ["en-US","fr-CA"]:
                guest.line("export LANG="+lang)
                result=sortie(guest,"sort --version")
                assert "0.1.0" in result,result
                results[lang]=sortie(guest,"sort --help")
                assert "--batch-size" in results[lang]
                guest.vm.stdin.write(b"man sort; echo MAN_RC=$?\n")
                guest.until(b"q quitter" if lang=="fr-CA" else b"q quit")
                guest.vm.stdin.write(b"q")
                guest.until(b"MAN_RC=0");guest.until(b"nsh> ")
            for cmd in ["sort /documents/exemple.txt","cat /documents/exemple.txt | sort -n","sort /documents/exemple.txt | head -n 1"]:
                guest.vm.stdin.write(f"nsh -c '{cmd}'; echo SORT_RC=$?__fin\n".encode())
                result=guest.until(b"__fin\r\n")
                rest=guest.until(b"nsh> ")
                # The UART also echoes the command. Wait for the executed status.
                import re
                raw=result+rest
                if not re.search(rb"SORT_RC=\d+__fin",raw):
                    raw+=guest.until(b"__fin\r\n")+guest.until(b"nsh> ")
                match=re.search(rb"SORT_RC=(\d+)__fin",raw)
                assert match,raw
                results[cmd]=int(match[1])
            (folder/"results.json").write_text(json.dumps(dict(ram_bytes=ram,sector=sector,disk_bytes=disk.stat().st_size,results=results),ensure_ascii=False,indent=2)+"\n")
if __name__=="__main__":main()
