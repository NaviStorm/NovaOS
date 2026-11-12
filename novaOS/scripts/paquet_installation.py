"""Préparer une nouvelle VM UTM ou sélectionner son disque système après installation."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile

from verifier_gpt import MIO, verifier_gpt

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
from profils_utm import disque_sequentiel, profil
from prototype_uefi import image


def construire(fv, elf, destination):
    destination.mkdir()
    with tempfile.TemporaryDirectory() as stage:
        raw = image(Path(stage) / "support", elf, "installer-gpt")
        subprocess.run(["qemu-img", "convert", "-c", "-f", "raw", "-O", "qcow2",
                        raw, destination / "installateur.qcow2"], check=True)
    shutil.copyfile(fv / "QEMU_EFI.fd", destination / "prototype.fd")
    shutil.copyfile(fv / "QEMU_VARS.fd", destination / "modele-vars.fd")


def systeme(paquet):
    """VM arrêtée, raccourci retiré ou UTM fermé. Changer uniquement le plist."""
    path = paquet / "config.plist"
    with path.open("rb") as f:
        config = plistlib.load(f)
    args = config["QEMU"]["AdditionalArguments"]
    source = disque_sequentiel("drivesource", 6, True)
    cible = disque_sequentiel("drivecible", 7, False)
    debut = next((i for i, a in enumerate(args) if a == "-fw_cfg"), None)
    prefixe = "name=opt/novaos/gpt-guids,string="
    lecteurs = [(d["Identifier"], d["ImageName"], d["ReadOnly"]) for d in config["Drive"]]
    attendus = [("code", "code.fd", True), ("variables", "variables.fd", False),
                ("source", "source.qcow2", True), ("cible", "cible.qcow2", False)]
    if (lecteurs != attendus or debut is None or len(args) < debut + 10
            or not args[debut+1].startswith(prefixe) or args[debut+2:debut+10] != source + cible):
        raise ValueError("La configuration ne correspond pas au profil d’installation")

    ids = bytes.fromhex(args[debut+1][len(prefixe):])
    disque = paquet / "Data" / "cible.qcow2"
    info = json.loads(subprocess.check_output(["qemu-img", "info", "-f", "qcow2", "--output=json", disque]))
    if info["virtual-size"] != 64*MIO or "backing-filename" in info:
        raise ValueError("La cible diffère du disque autonome de 64 Mio du profil graphique")
    with tempfile.TemporaryDirectory() as tmp:
        brut = Path(tmp) / "controle.raw"
        subprocess.run(["qemu-img", "convert", "-f", "qcow2", "-O", "raw", disque, brut], check=True)
        verifier_gpt(brut, 512, ids)

    systeme = disque_sequentiel("drivecible", 7, False, designation="NovaOS-System-v1")
    systeme[1] += ",bootindex=1"
    args[debut:debut+10] = systeme
    config["Drive"] = [d for d in config["Drive"] if d["Identifier"] != "source"]
    with tempfile.NamedTemporaryFile(dir=paquet, delete=False) as f:
        temp = Path(f.name)
        try:
            plistlib.dump(config, f)
            f.flush()
            os.fsync(f.fileno())
            os.replace(temp, path)
        finally:
            temp.unlink(missing_ok=True)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="action", required=True)
    b = sub.add_parser("construire", help="produire le support et ses fichiers firmware")
    b.add_argument("fv", type=Path)
    b.add_argument("elf", type=Path)
    b.add_argument("destination", type=Path)
    n = sub.add_parser("creer", help="créer exclusivement une nouvelle VM et un disque vierge")
    n.add_argument("distribution", type=Path)
    n.add_argument("destination", type=Path, help="nouveau répertoire hôte")
    s = sub.add_parser("systeme", help="après arrêt et retrait du raccourci UTM, sélectionner le disque installé")
    s.add_argument("paquet", type=Path)
    a = p.parse_args()
    if a.action == "construire":
        construire(a.fv, a.elf, a.destination)
    elif a.action == "creer":
        a.destination.mkdir()
        profil(a.destination, a.distribution, a.distribution / "installateur.qcow2", True, installation=True)
    else:
        systeme(a.paquet)


if __name__ == "__main__":
    main()
