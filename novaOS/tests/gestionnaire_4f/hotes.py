"""Non-régression 4B–4E.1 et séparation des paquets invités entre les ISA."""
import argparse
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib

NOVA = Path(__file__).resolve().parents[2]
REF = "b0ae035cbd9fd6a5ad5cf3fa9be2f986e401e18a"


def run(*args):
    subprocess.run(args, cwd=NOVA.parent, check=True)


def chargeur(path):
    spec = importlib.util.spec_from_file_location("paquet_de_test", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.build


def paquets(tmp, arm, q35):
    prep = NOVA/"user/gestionnaire-4d"
    before, after = tmp/"origine", tmp/"actuel"
    before.mkdir()
    after.mkdir()
    for name in ("preparer.py","pilote.toml.in"):
        (before/name).write_bytes(subprocess.check_output(
            ["git","show",f"{REF}:novaOS/user/gestionnaire-4d/{name}"], cwd=NOVA.parent))
    original, current = chargeur(before/"preparer.py"), chargeur(prep/"preparer.py")
    original(arm,before)
    current(arm,after,"aarch64")
    for f in after.iterdir():
        if f.read_bytes() != (before/f.name).read_bytes():
            raise ValueError(f"Objet ARM divergent sur les mêmes octets ELF : {f.name}")
    current(q35,after,"x86_64")
    for data,isa in ((arm,"x86_64"),(q35,"aarch64")):
        try:
            current(data,after,isa)
        except ValueError:
            continue
        raise ValueError("Confusion de type de machine")
    print("Six objets ARM identiques à 4E.1 sur le même ELF. Les deux croisements ISA sont refusés.", flush=True)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("elf_arm",type=Path)
    p.add_argument("elf_q35",type=Path)
    a=p.parse_args()
    dest=Path(os.environ["CARGO_TARGET_DIR"]).resolve()
    if Path(os.environ["NOVAOS_TMPDIR"]).resolve() not in dest.parents:
        raise ValueError("Répertoire privé requis")
    run(sys.executable,"-B",NOVA/"tests/gestionnaire_4d/hotes.py")
    with tempfile.TemporaryDirectory(prefix="hotes-4f-",dir=dest) as td:
        tmp=Path(td)
        for isa in ("aarch64-apple-darwin","x86_64-apple-darwin"):
            exe=tmp/isa
            run("rustup","run","stable","rustc","--edition","2024","--test","--target",isa,
                NOVA/"user/gestionnaire-4d/reprise_etat.rs","-o",exe)
            verdict=subprocess.check_output([exe],text=True)
            if "3 passed; 0 failed" not in verdict:
                raise ValueError(verdict)
            print(isa,verdict,flush=True)
        paquets(tmp,a.elf_arm.read_bytes(),a.elf_q35.read_bytes())
    run("git","diff","--exit-code","--quiet",REF,"HEAD","--","novaOS/abi","novaOS/Cargo.toml",
        "novaOS/user/ethernet","novaOS/user/virtio_net","novaOS/user/novaauth","novaFS","components")
    if len(tomllib.loads((NOVA/"abi/syscalls.toml").read_text())["call"])!=48:
        raise ValueError("Registre ABI")
    print("48 sélecteurs. Contrat Ethernet, NovaFS et manifeste interne identiques à 4E.1.",flush=True)


if __name__=="__main__":
    main()
