"""Construire et qualifier le profil ARM 4D. Le lanceur 3E détient le verrou QEMU partagé."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys

NOVA = Path(__file__).resolve().parents[2]
REPO = NOVA.parent
BASE = REPO.parent
PORT = NOVA / "tests/ethernet_v1/port_qemu.py"
ISA = "aarch64-unknown-none-softfloat"
NEG = {"signature", "revision", "racines", "lancement","attribution","droits"}
CASES = {"plein","choix-data","reset-court","reset-deces","reset-revocation","reset-incertain","reset-absent",
         "reset-iommu-absent","reset-iommu-differe","cycles-256","echo1514-reset","temoin-3e",*NEG}


def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f,"sha256").hexdigest()


def output(*args):
    return subprocess.check_output(args,cwd=REPO,text=True).strip()


def write(path, data):
    path.write_text(json.dumps(data,ensure_ascii=False,indent=2)+"\n")


def elf(path):
    b = path.read_bytes()
    at = struct.unpack_from("<Q",b,32)[0]
    n = struct.unpack_from("<H",b,56)[0]
    segments = []
    for i in range(n):
        typ,flags,_,va,_,filesz,memsz,_=struct.unpack_from("<II6Q",b,at+56*i)
        if typ == 1:
            segments.append(dict(flags=flags, fichier=filesz, memoire=memsz,
                                 cadres=((va%4096 + memsz+4095)//4096)))
    return dict(sha256=sha(path),octets=len(b),segments=segments)


def build(case, archive):
    if case=="temoin-3e":
        path=BASE/"archives-ethernet-phase3e/images-qualifiees/echo1514-reset/arm.elf"
        return path, json.loads(path.with_name("construction.json").read_text())["commit"]
    folder=archive/"images"/case
    ref=output("git","rev-parse","HEAD")
    k=folder/"arm.elf"
    if k.exists():
        manifest=json.loads((folder/"construction.json").read_text())
        if manifest["commit"] != ref or sha(k) != manifest["elf"]["arm"]:
            raise ValueError("révision ou empreinte différente dans l’archive")
        return k,ref
    folder.mkdir(parents=True,exist_ok=True)
    target=Path(os.environ["CARGO_TARGET_DIR"]).resolve()
    if Path(os.environ["NOVAOS_TMPDIR"]).resolve() not in target.parents:
        raise ValueError("CARGO_TARGET_DIR privé requis sous NOVAOS_TMPDIR")
    env=dict(os.environ,RUSTC=output("rustup","which","--toolchain","stable","rustc"),
             CARGO_PROFILE_RELEASE_STRIP="none",NOVAOS_ETH3_CASE="reset-court" if case in NEG else case,
             NOVAOS_4D_REFUS=case if case in NEG else "")
    cmd=["rustup","run","stable","cargo","build","--locked","--offline","--release","--features","gestionnaire-4d","--target",ISA]
    with (folder/"construction.log").open("w") as log:
        subprocess.run(cmd,cwd=NOVA,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    source=target/ISA/"release"
    metadata=list(source.glob("build/novaos-*/out/gestionnaire.elf"))
    if len(metadata)!=1:
        raise ValueError("plusieurs sorties 4D, choisir un répertoire privé")
    out=metadata[0].parent
    actors={p:elf(out/p) for p in ("gestionnaire.elf","pair.elf","net_preparation.bin")}
    pkg=(out/"pilote.pkg01").read_bytes()
    offset=struct.unpack_from("<I",pkg)[0]+68
    if pkg[offset:]!=(out/"net_preparation.bin").read_bytes():
        raise ValueError("ELF du contrat différent de la charge du paquet")
    shutil.copyfile(source/"novaos",k)
    write(folder/"construction.json",dict(commit=ref,elf={"arm":sha(k)},acteurs=actors,commande=cmd,
         cas=env["NOVAOS_ETH3_CASE"],injection=env["NOVAOS_4D_REFUS"],rustc=output(env["RUSTC"],"-V"),
         paquet={"taille":len(pkg),"sha256":hashlib.sha256(pkg).hexdigest(),"manifeste":sha(out/"pilote.toml")},
         sources_adaptateur=output("git","rev-parse","HEAD:novaOS/user/ethernet/adaptateur.rs"),
         sources_courtier=output("git","rev-parse","HEAD:novaOS/user/virtio_net/ethernet_lab.rs")))
    return k,ref


def observer(case, dest, report):
    if not report["succes"] or report["elf_relu_esp"]!=report["elf"]:
        raise ValueError("échec du banc 3E ou de la relecture EFI")
    text=(dest/"serie.log").read_text()
    if case=="temoin-3e": return
    if case in NEG:
        if report["refus_4d"]["cause"]!=case: raise ValueError("cause de refus absente")
        return
    launches=re.findall(r"D4 lancement scelle epoch=(\d+) pid=(\d+) incarnation=(\d+) generation=(\d+)",text)
    controls=re.findall(r"D4 controle epoch=(\d+) recus=true acteurs=\[Exit\(0\), Exit\(0\)\] mode=0",text)
    balances=re.findall(r"D4 cycle avant=(\d+) minimum=(\d+) apres=(\d+) pic=(\d+) ipc=(\d+) dma=(\d+)",text)
    closes=re.findall(r"RAM phase=4d_fin_cycle .*?ipc=0 utilisateur=0 processus=0 ",text)
    wanted=256 if case=="cycles-256" else 2 if case in ("reset-court","reset-iommu-differe") else 1
    if len(launches)!=wanted or len(balances)!=wanted or len(closes)!=wanted or controls!=[x[0] for x in launches]:
        raise ValueError("fermetures, admissions ou contrôleurs résiduels")
    if any(len({x[i] for x in launches})!=wanted for i in range(4)):
        raise ValueError("réemploi du titulaire, de l’incarnation ou du bail")
    if case=="cycles-256" and any(a!=z or ipc!="0" or dma!="16" for a,_,z,_,ipc,dma in balances):
        raise ValueError("dérive mémoire, IPC ou DMA après un cycle nominal")
    if (text.count("avant_capacites=true")!=wanted or text.count("D4 lanceur prepare=1 creation=1 recu=consulte rejeu=sans_effet ack=4")!=wanted
            or text.count("racines=[3, 3] boites=0 cloture=independante")!=wanted):
        raise ValueError("reçu de lancement, idempotence ou racines de contrôle")
    if text.count("ancienne_capacite_valide=false")!=wanted:
        raise ValueError("contrôle des anciennes capacités absent")
    report["dynamique"]=dict(cycles= wanted,pic_cadres=max(int(b[3]) for b in balances),
        fermetures_sans_processus=len(closes),ipc_final=int(balances[-1][4]),dma_final_cycle=int(balances[-1][5]))
    write(dest/"campagne.json",report)


def run(case, number, root):
    image,ref=build(case,root)
    dest=root/case/str(number)
    if (dest/"campagne.json").exists():
        report=json.loads((dest/"campagne.json").read_text())
        if report["elf"]!=sha(image): raise ValueError("preuve existante pour un autre ELF")
        observer(case,dest,report)
        return
    files=BASE/"archives-ethernet-phase3/reproduction-8bd3f9c"
    actual = "port" if case in NEG else "echo1514-reset" if case=="temoin-3e" else case
    cmd=[sys.executable,"-B",PORT,"arm",actual,image,dest,BASE/"pilote-virtio-net",BASE/"archives-virtio-net",
         files/"limine-enroll",files/"BOOTAA64.EFI","--limine-arm",files/"limine-a72.EFI","--source-elf",ref,
         "--cycle-vie","--esp-verifiee","--delai", "600" if case=="cycles-256" else "120"]
    if case!="temoin-3e": cmd+=["--gestionnaire-4d"]
    if case in NEG: cmd+=["--refus-4d",case]
    subprocess.run(cmd,cwd=REPO,check=True)
    observer(case,dest,json.loads((dest/"campagne.json").read_text()))
    trace=dest/"qemu.trace"
    if trace.stat().st_size > 12<<20:
        with trace.open("rb") as src,gzip.open(dest/"qemu.trace.gz","wb",compresslevel=3) as dst:
            shutil.copyfileobj(src,dst)
        # Vérifier avant de retirer uniquement la trace nouvellement produite.
        with gzip.open(dest/"qemu.trace.gz","rb") as src:
            if hashlib.file_digest(src,"sha256").hexdigest()!=sha(trace): raise ValueError("trace incomplète")
        trace.unlink()


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive",type=Path)
    p.add_argument("cas",nargs="+",choices=sorted(CASES))
    p.add_argument("--demarrages",type=int,choices=range(1,4),default=1)
    a=p.parse_args()
    if output("git","status","--porcelain","--untracked-files=no"):
        raise SystemExit("Indexer la révision source avant de constituer les preuves")
    failed=[]
    for n in range(1,a.demarrages+1):
        for case in a.cas:
            print(f"Début {case} démarrage {n}",flush=True)
            try:
                run(case,n,a.archive.resolve())
                print(f"Validé {case} démarrage {n}",flush=True)
            except (OSError,ValueError,subprocess.CalledProcessError,KeyError) as e:
                failed.append((case,n,str(e)))
                print(f"Échec {case} démarrage {n} {e}",flush=True)
    if failed: raise SystemExit(1)

if __name__=="__main__": main()
