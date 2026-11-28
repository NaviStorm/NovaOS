"""Observer le PE 5R sur l'OVMF USB de référence, sans émetteur du contrat 5Q."""

import argparse
import json
import subprocess
import sys
import tarfile
import zipfile
from hashlib import sha256
from pathlib import Path

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5o"))
from archiver import png
from demarrage import FIRMWARE, STORES, boot, public_member
from image5o import MIO, creer, lecteur, verifier

sys.path.insert(0, str(NOVA / "tests/ethernet_v1"))
from port_qemu import qualification

PE = "7f80bf3c42c89162d67fb229b9080e9706c0f7f6a3574978051c1c0c555477a5"
IMAGE = "9440a1030b530174d97e85e4e55cb6566c514dfd3a143a68fff77ae8b5f1b404"


def empreinte(data):
    return sha256(data).hexdigest()


def exiger(condition, message):
    if not condition:
        raise ValueError(message)


def lire_ref(path, attendu):
    with lecteur(path) as f:
        data = f.read()
    exiger(empreinte(data) == attendu, "Référence différente : " + path.name)
    return data


def mesurer(pe_path, magasin, nom):
    exiger(nom.isascii() and nom.isalnum(), "Nom de série simple requis")
    out = NOVA / "build" / ("qualification-5s-" + nom)
    out.mkdir(parents=True, exist_ok=False, mode=0o700)
    pe = lire_ref(pe_path, PE)
    variables = json.loads(
        (NOVA / "docs/validation/diagnostic-uefi-5r/variables.json").read_bytes()
    )
    actif = lire_ref(magasin, variables["copie"])
    inactif = public_member("inactif.fd", STORES["inactif"])
    fw = out / "code.fd"
    with tarfile.open(
        NOVA / "docs/validation/diagnostic-uefi-5e/laboratoire.tar.gz", "r:gz"
    ) as t:
        data = t.extractfile("ovmf/code.fd").read()
        exiger(empreinte(data) == FIRMWARE, "Firmware hors référence")
        fw.write_bytes(data)
    medium = out / "usb.img"
    creer(medium, 68 * MIO, pe)
    lire_ref(medium, IMAGE)
    version = subprocess.check_output(
        ["qemu-system-x86_64", "--version"], text=True
    ).splitlines()[0]
    exiger(version.startswith("QEMU emulator version 11.1.3"), "Version QEMU")
    with (
        qualification(NOVA.parent.parent, {}),
        zipfile.ZipFile(out / "temoin.zip", "x") as archive,
    ):
        for mode, store in (("actif", actif), ("inactif", inactif)):
            args = {"image_test": pe, "variables_test": store, "ecran": True}
            with lecteur(medium) as src:
                verifier(src, 68 * MIO, pe)
                base = boot(out / ("cal-" + mode), fw, mode, src, **args)["base"]
                dossier = out / mode
                bilan = boot(dossier, fw, mode, src, base, **args)
                verifier(src, 68 * MIO, pe)
            lire_ref(medium, IMAGE)
            debut = (dossier / "avant.ppm").read_bytes()
            fin = (dossier / "apres.ppm").read_bytes()
            exiger(debut == fin and bilan["interne_intact"], "Écran ou canari modifié")
            bilan.update(
                firmware=FIRMWARE,
                magasin_initial=empreinte(store),
                image=IMAGE,
                qemu=version,
                sha256_ppm_entree=empreinte(debut),
                sha256_ppm_hlt=empreinte(fin),
            )
            for piece, contenu in (
                (
                    "observation.json",
                    (json.dumps(bilan, indent=2, sort_keys=True) + "\n").encode(),
                ),
                ("entree.png", png(debut)),
                ("hlt.png", png(fin)),
                (
                    "commandes-intervalle.log",
                    (dossier / "transactions.log").read_bytes(),
                ),
            ):
                if piece.endswith(".log"):
                    exiger(
                        len(contenu) == bilan["trace_commandes_sata_octets"] == 0,
                        "Fenêtre de trace non bornée",
                    )
                info = zipfile.ZipInfo(mode + "/" + piece, (2000, 1, 1, 0, 0, 0))
                info.external_attr = 0o100644 << 16
                archive.writestr(info, contenu, zipfile.ZIP_DEFLATED)
            print(
                mode, bilan["verdict"], bilan["compteurs_entree"]["interne"], flush=True
            )
    lire_ref(magasin, variables["copie"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pe", type=Path)
    parser.add_argument("magasin_actif_5r", type=Path)
    parser.add_argument("serie")
    args = parser.parse_args()
    mesurer(args.pe, args.magasin_actif_5r, args.serie)
