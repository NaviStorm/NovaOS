"""QEMU uniquement. Deux profils indépendants. Les répertoires de résultats doivent être neufs."""
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
from contextlib import ExitStack
from types import SimpleNamespace

ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from cli import Terminal
from bloc import qmp
from journalisation import attend, sortie
from session_cli import fixture_p9
from threads import exercise_example

def document(arch,sector,out):
    out.mkdir(parents=True)
    disk=out/"volume.raw"
    fixture_p9(disk,sector)
    with ExitStack() as resources:
        client,peer=socket.socketpair()
        resources.enter_context(client)
        resources.enter_context(peer)
        channel=resources.enter_context(client.makefile("rwb",buffering=0))
        monitor=SimpleNamespace(stdin=channel,stdout=channel)
        guest=resources.enter_context(Terminal(arch,sector,disk,"sed-doc",journal=out/"uart.serial",qmp_bloc=peer.fileno()))
        peer.close()
        qmp(monitor,"qmp_capabilities")
        guest.until(b"nsh> ")
        attend(lambda: sortie(guest,"service status 16"),lambda text:"Current state: RUNNING" in text)
        def snapshot():
            stats,=[row["stats"] for row in qmp(monitor,"query-blockstats") if row["device"]=="d"]
            return [hashlib.sha256(disk.read_bytes()).hexdigest(),stats["wr_bytes"],stats["wr_operations"],stats["flush_operations"]]
        for _ in range(15):
            before=snapshot();time.sleep(2)
            if before==snapshot():break
        else:raise AssertionError("P9 encore actif")
        rows=[]
        for tag,fr in [("en-US",False),("fr-CA",True),("fr--CA",False)]:
            guest.line(f"export LANG={tag}")
            wording="Éditer un flux" if fr else "Edit a stream"
            for command in ["sed --help","sed -i --help","sed --version"]:
                text=sortie(guest,command)
                assert ("NovaOS sed 0.1.0" if command.endswith("--version") else wording) in text,text
                rows.append([tag,command,0])
            guest.vm.stdin.write(b"man sed; echo man-status=$?\n")
            frame=guest.until(b"q quitter" if fr else b"q quit  / search")
            assert wording.encode() in frame,frame
            guest.vm.stdin.write(b"q")
            guest.until(b"man-status=0");guest.until(b"nsh> ")
            rows.append([tag,"man sed",0])
        for cmd in ["sed -i 1d /documents/exemple.txt","sed 1q /documents/exemple.txt"]:
            result=sortie(guest,cmd,4)
            assert ("P9" if " -i " in cmd else "catalogue") in result,result
            rows.append(["fr--CA",cmd,4])
        for cmd in ["cat /documents/exemple.txt | sed -n 1p", "cat /documents/exemple.txt | sed 1q",
                    "sed -n 1,3p /documents/exemple.txt | wc -l"]:
            guest.vm.stdin.write(f"nsh -c '{cmd}'; echo SED_RC_$?__fin\n".encode())
            guest.until(b"SED_RC_3__fin")
            guest.until(b"nsh> ")
            rows.append(["fr--CA",cmd,3])
        after=snapshot()
        assert before==after,(before,after)
    return dict(sector=sector,before=before,after=after,cases=rows)

def main(arch,out):
    out.mkdir(parents=True)
    target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[arch]
    build_dir=Path(os.environ["NOVAOS_TMPDIR"])/("arc027-sdk-sed-"+arch)
    os.environ.update(NOVA_ARCH=arch,NOVA_VCPUS="2",NOVA_RAM="256",CARGO_TARGET_DIR=str(build_dir),
                      RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    root=ROOT/"components/sed"
    elf=Path(subprocess.check_output([root/"build.sh",arch,"--features","qualification"],text=True).strip().splitlines()[-1])
    os.environ["NOVA_THREAD_PROBE_ELF"]=str(elf)
    subprocess.run(["rustup","run","stable","cargo","build","-p","novaos","--target",target,"--release","--locked","--offline","--features","thread-probe"],cwd=ROOT/"novaOS",check=True)
    os.environ["NOVA_KERNEL_ELF"]=str(build_dir/target/"release/novaos")
    proof=exercise_example(arch,2)
    (out/"sdk.json").write_text(json.dumps(dict(core=proof,witness_sha256=hashlib.sha256(elf.read_bytes()).hexdigest()),indent=2)+"\n")
    # Les consultations sont rejouées sur le noyau ordinaire.
    del os.environ["NOVA_KERNEL_ELF"]
    subprocess.run(["make","-C",str(ROOT/"novaOS"),"ARCH="+arch,"kernel"],check=True,env={k:v for k,v in os.environ.items() if k!="CARGO_TARGET_DIR"})
    for sector in [512,4096]:
        folder=out/str(sector)
        result=document(arch,sector,folder)
        (folder/"results.json").write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")

if __name__=="__main__":main(sys.argv[1],Path(sys.argv[2]).resolve())
