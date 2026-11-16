"""Rejouer le témoin du catalogue CUT avec nsh et un P9 jetable. Utiliser un dossier neuf."""
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
VERSION = (ROOT / "components/cut/VERSION").read_bytes().strip()
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from cli import Terminal
from pipe_natif import fixture
from journalisation import arret

def digest(path):
    with path.open("rb") as stream:return hashlib.file_digest(stream,"sha256").hexdigest()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("sector",type=int,choices=[512,4096])
    p.add_argument("out",type=Path)
    a=p.parse_args()
    out=a.out.resolve()
    out.mkdir(parents=True,exist_ok=False)
    target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[a.arch]
    tmp=Path(os.environ["NOVAOS_TMPDIR"])
    env=dict(os.environ,CARGO_TARGET_DIR=str(tmp/f"cut-0240-kernel-{a.arch}"))
    env["RUSTC"]=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip()
    subprocess.run(["rustup","run","stable","cargo","build","--release","--locked","--offline","--target","aarch64-apple-darwin","--bin","novafs"],cwd=ROOT/"novaFS",check=True,env={**env,"CARGO_TARGET_DIR":str(ROOT/"novaFS/target")})
    subprocess.run(["make","-C",str(ROOT/"novaOS"),"ARCH="+a.arch,"kernel"],check=True,env=env)
    kernel=Path(env["CARGO_TARGET_DIR"])/target/"release/novaos"
    os.environ.update(NOVA_RAM="256",NOVA_VCPUS="2",NOVA_KERNEL_ELF=str(kernel))
    (ROOT/"novaOS/build").mkdir(exist_ok=True)
    image,_,_=fixture(out,a.sector,{"donnees.tsv":b"1\t2\t3\t4\t5\t6\t7\t8\t9\n"})
    cases=[]
    serial=out/"nsh.serial"
    with Terminal(a.arch,a.sector,image,"cut-catalogue",test=False,journal=serial) as vm:
        vm.until(b"nsh> ")
        def run(cmd,manual=False):
            vm.vm.stdin.write(b"echo CUTDEBUT\n")
            vm.until(b"\r\nCUTDEBUT\r\n")
            vm.until(b"nsh> ")
            vm.vm.stdin.write((cmd+"\n").encode())
            raw=b""
            if manual:
                raw=vm.until(b"man cut 1/")
                vm.vm.stdin.write(b"q")
            raw+=vm.until(b"nsh> ")
            vm.vm.stdin.write(b"echo CF_$?__fin\n")
            raw+=vm.until(b"__fin\r\n")
            if not re.search(rb"CF_(\d+)__fin",raw):raw+=vm.until(b"__fin\r\n")
            vm.until(b"nsh> ")
            status=int(re.search(rb"CF_(\d+)__fin",raw)[1])
            cases.append(dict(commande=cmd,statut=status,serie_hex=raw.hex()))
            return raw,status
        for cmd in ["cut -f1,3,8,9 donnees.tsv","cat donnees.tsv | cut -f1,3,8,9",
                    "cut --inconnue fichier-absent","cut --help","cut --version","cat donnees.tsv | wc -l"]:
            run(cmd)
        for tag in ["fr-CA","fr--CA","en-US"]:
            run("export LANG="+tag)
            run("man cut",True)
        for tag in ["fr-CA","fr--CA"]:
            run("export LANG="+tag)
            import time
            time.sleep(1)
            before=digest(image)
            help_text,help_rc=run("cut --help")
            version,version_rc=run("cut --version")
            after=digest(image)
            assert help_rc==version_rc==0 and VERSION in version
            assert ("Extraire".encode() in help_text)==(tag=="fr-CA")
            cases.append(dict(commande="P9 documentation "+tag,statut=0,avant=before,apres=after))
            assert before==after
        _,missing=run("cut donnees.tsv")
        assert missing!=0
        arret(vm,2)
    # Relire l'ELF contenu dans le média que QEMU vient d'utiliser.
    extracted=out/"demarre.elf"
    iso=serial.with_suffix(".iso")
    subprocess.run(["xorriso","-osirrox","on","-indev",iso,"-extract","/boot/novaos.elf",extracted],check=True)
    assert digest(extracted)==digest(kernel)
    raw=extracted.read_bytes()
    build=kernel.parent/"build"
    cli_dirs=[p.parent for p in build.glob("novaos-*/out/shell.elf") if p.read_bytes() in raw]
    assert len(cli_dirs)==1
    embedded={p.name:digest(p) for p in sorted(cli_dirs[0].glob("*.elf")) if p.read_bytes() in raw}
    assert "shell.elf" in embedded and "cat.elf" in embedded
    cut=ROOT/f"livraison-cut-0240/bin/cut-{a.arch}.elf"
    info=dict(architecture=a.arch,secteur=a.sector,ram_mio=256,vcpu=2,iso_sha256=digest(iso),
              noyau_sha256=digest(kernel),elf_embarques=embedded,cut_sha256=digest(cut),
              cut_embarque=cut.read_bytes() in raw,commandes=cases)
    (out/"resultats.json").write_text(json.dumps(info,ensure_ascii=False,indent=2)+"\n")
    print(a.arch,a.sector,[(c["commande"],c["statut"]) for c in cases],flush=True)
    assert cases[5]["statut"]==0
    assert all(cases[i]["statut"]==0 for i in (7,9,11))
    assert all(cases[i]["statut"]!=0 for i in range(3)) and all(cases[i]["statut"]==0 for i in (3,4)) and not info["cut_embarque"], "Catalogue modifié : requalifier les ELF et le pipe"
if __name__=="__main__":main()
