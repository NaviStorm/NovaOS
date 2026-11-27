"""Mesurer les ELF liés, les copies immuables et les invariants de la phase 4D."""
import argparse
import json
from pathlib import Path
import shutil
import struct
import subprocess
import tomllib

from bilan import exiger
from qualifier import REPO, elf, sha, write


def chargeables(b):
    off = struct.unpack_from("<Q", b, 32)[0]
    pas, nombre = struct.unpack_from("<HH", b, 54)
    for i in range(nombre):
        typ, _, debut, _, _, taille, _, _ = struct.unpack_from("<II6Q", b, off + i * pas)
        if typ == 1:
            yield b[debut:debut + taille]


def mesures(args):
    ref = "34bb4e6a49eb28999665821496e7923e8b1fe557"
    def git(*params):
        return subprocess.check_output(["git", *params], cwd=REPO, text=True).strip()
    version = git("rev-parse", "HEAD")
    proteges = ["novaOS/abi", "novaOS/Cargo.toml", "novaOS/kernel/src/hardware/net_q35",
                "novaOS/kernel/src/hardware/net_q35.rs", "novaOS/kernel/src/hardware/nvme_domain",
                "novaOS/kernel/src/hardware/nvme_domain.rs", "novaOS/kernel/src/hardware/block/vtd.rs",
                "novaOS/user/ethernet", "novaOS/user/virtio_net", "novaOS/user/novaauth", "novaFS", "components"]
    subprocess.run(["git", "diff", "--exit-code", "--quiet", ref, version, "--", *proteges], cwd=REPO, check=True)
    abi = REPO / "novaOS/abi/syscalls.toml"
    exiger(len(tomllib.loads(abi.read_text())["call"]) == 48, "registre ABI")
    exiger("pub const CREDITS: usize = 8;" in (REPO / "novaOS/user/ethernet/backend.rs").read_text(), "crédits")
    out = args.archive / "objets"
    out.mkdir(exist_ok=True)
    noms = ("pilote.pkg01", "pilote.toml", "pilote.sig", "profil.bin", "profil.toml", "profil.sig",
            "net_preparation.bin", "gestionnaire.elf", "pair.elf")
    for nom in noms:
        shutil.copyfile(args.objets / nom, out / nom)
    construction = json.loads((args.archive / "images/echo1514-reset/construction.json").read_text())
    exiger(construction["commit"] == version and construction["paquet"]["sha256"] == sha(out / "pilote.pkg01"),
           "paquet distinct du démarrage")
    for nom, infos in construction["acteurs"].items():
        exiger(infos == elf(out / nom), nom)
    services = {n: elf(args.objets / n) for n in ("ethernet_port.bin", "ethernet_client.bin")}
    noyaux = {
        "dynamique": args.archive / "images/echo1514-reset/arm.elf",
        "statique": args.archive / "images/temoin-meme-revision/arm.elf",
        "historique": args.historique,
    }
    donnees = {}
    nm = shutil.which("llvm-nm") or "/opt/homebrew/opt/llvm/bin/llvm-nm"
    for role, path in noyaux.items():
        segments = list(chargeables(path.read_bytes()))
        symboles = {}
        for ligne in subprocess.check_output([nm, "-n", path], text=True).splitlines():
            xs = ligne.split()
            if len(xs) == 3 and xs[2] in ("__image_start", "__image_end"):
                symboles[xs[2]] = int(xs[0], 16)
        cadres, reste = divmod(symboles["__image_end"] - symboles["__image_start"], 4096)
        exiger(reste == 0, "bornes noyau non alignées")
        copies = {n: sum(seg.count((out / n).read_bytes()) for seg in segments) for n in noms}
        if role == "dynamique":
            for n in services:
                exiger(sum(seg.count((args.objets / n).read_bytes()) for seg in segments) == 1,
                       f"ELF du port embarqué différent {n}")
        donnees[role] = dict(elf=elf(path), cadres_image=cadres, occurrences_dans_PT_LOAD=copies)
    exiger(donnees["historique"]["occurrences_dans_PT_LOAD"]["net_preparation.bin"] == 1,
           "le témoin ne contient pas exactement ce pilote")
    x86 = {}
    for nom in ("gestionnaire", "pair"):
        path = args.acteurs_x86 / nom
        exiger(struct.unpack_from("<H", path.read_bytes(), 18)[0] == 62, "ELF64 x86_64 attendu")
        x86[nom] = elf(path)
    sortie = dict(revision=version, reference_invariants=ref, selec_abi=48, sha256_registre=sha(abi),
                  credits_ethernet=8, sources_verifiees=proteges, noyaux=donnees,
                  acteurs_arm=construction["acteurs"], service_et_client_arm=services, paquet=construction["paquet"], acteurs_nus_x86=x86)
    write(args.archive / "hotes/objets.json", sortie)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for arg in ("archive", "objets", "historique", "acteurs_x86"):
        p.add_argument(arg, type=Path)
    mesures(p.parse_args())
