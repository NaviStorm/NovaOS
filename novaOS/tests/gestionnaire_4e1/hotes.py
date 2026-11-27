"""Régression du catalogue, du gestionnaire et du décodage des vues de reprise."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import tomllib

NOVA = Path(__file__).resolve().parents[2]
BASE = "de70363f4eab48ccfea11963951451593f37cc59"

def run(*cmd,**options):
    subprocess.run(cmd,cwd=NOVA.parent,check=True,**options)

def main():
    racine=Path(os.environ["NOVAOS_TMPDIR"]).resolve()
    cibles=Path(os.environ["CARGO_TARGET_DIR"]).resolve()
    if racine not in cibles.parents: raise ValueError("Répertoire privé de qualification requis")
    run(sys.executable,"-B",NOVA/"tests/gestionnaire_4d/hotes.py")
    protections=["novaOS/abi","novaOS/Cargo.toml","novaOS/kernel/src/hardware/net_q35",
                 "novaOS/kernel/src/hardware/net_q35.rs","novaOS/kernel/src/hardware/nvme_domain",
                 "novaOS/kernel/src/hardware/nvme_domain.rs","novaOS/kernel/src/hardware/block/vtd.rs",
                 "novaOS/user/ethernet","novaOS/user/virtio_net","novaOS/user/novaauth","novaFS","components"]
    run("git","diff","--exit-code","--quiet",BASE,"HEAD","--",*protections)
    if len(tomllib.loads((NOVA/"abi/syscalls.toml").read_text())["call"])!=48:
        raise ValueError("Registre ABI")
    print("48 sélecteurs et composants protégés identiques à la livraison 4D",flush=True)
    source=NOVA/"user/gestionnaire-4d/reprise_etat.rs"
    with tempfile.TemporaryDirectory(prefix="vues-4e1-",dir=cibles) as tmp:
        for isa in ("aarch64-apple-darwin","x86_64-apple-darwin"):
            print(f"Vues de reprise sur {isa}",flush=True)
            exe=Path(tmp)/isa
            run("rustup","run","stable","rustc","--edition","2024","--test","--target",isa,source,"-o",exe)
            run(exe)

if __name__=="__main__":main()
