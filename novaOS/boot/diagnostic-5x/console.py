"""Un démarrage OVMF avec perte simulée de ConOut à la première instruction du PE."""

import argparse
import importlib.util
import json
from pathlib import Path
import re
import struct
import subprocess
import sys
import zlib

from virt.firmware.varstore.edk2 import Edk2VarStore

NOVA = Path(__file__).resolve().parents[2]
STANDARD = NOVA / "boot/diagnostic-5w"
sys.path.insert(0, str(STANDARD))
spec = importlib.util.spec_from_file_location("banc_standard", STANDARD / "banc.py")
banc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(banc)


def sans_console(debug):
    # À l'entrée PE32+ x64, le deuxième argument (EFI_SYSTEM_TABLE) arrive dans RDX.
    regs = bytes.fromhex(debug.command("g"))
    table = struct.unpack_from("<Q", regs, 24)[0]
    taille = 120  # sizeof(EFI_SYSTEM_TABLE), ABI UEFI x86_64
    if table % 8 or not 0 < table <= banc.RAM - taille:
        raise ValueError("Table système hors RAM de la VM jetable")
    avant = bytes.fromhex(debug.command(f"m{table:x},{taille:x}"))
    if len(avant) != taille or avant[:8] != b"IBI SYST":
        raise ValueError("Signature EFI_SYSTEM_TABLE absente")
    taille_entete, crc = struct.unpack_from("<II", avant, 12)
    if taille_entete != taille or zlib.crc32(avant[:16] + bytes(4) + avant[20:]) != crc:
        raise ValueError("Taille ou CRC initial du firmware")
    if not all(struct.unpack_from("<QQ", avant, 56)):
        raise ValueError("Interface de console initialement absente")
    # Panne de banc. Seuls ConsoleOutHandle, ConOut et le CRC de cet objet simulé changent.
    apres = bytearray(avant)
    apres[56:72] = bytes(16)
    apres[16:20] = bytes(4)
    struct.pack_into("<I", apres, 16, zlib.crc32(apres))
    for offset, longueur in ((56, 16), (16, 4)):
        valeur = apres[offset:offset + longueur].hex()
        if debug.command(f"M{table + offset:x},{longueur:x}:{valeur}") != "OK":
            raise ValueError("Simulation de perte de console refusée")
    if bytes.fromhex(debug.command(f"m{table:x},{taille:x}")) != apres:
        raise ValueError("Table système simulée non confirmée")
    return {"interface": "ConOut", "presente_avant": True, "presente_apres": False,
            "crc_recalcule": True}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for nom in ("firmware", "supports", "sortie"):
        p.add_argument(nom, type=Path)
    args = p.parse_args()
    fw, supports, sortie = (x.resolve() for x in (args.firmware, args.supports, args.sortie))
    ref = json.loads((NOVA / "docs/validation/diagnostic-uefi-5w/mesures.json").read_text())
    image = supports / "standard.img"
    magasin = supports / "actif.fd"
    if not all(p.is_file() for p in (fw, image, magasin)):
        raise ValueError("Trois fichiers ordinaires du banc requis")
    if banc.digest(fw) != ref["ovmf"] or banc.digest(image) != ref["image"]:
        raise ValueError("Firmware ou image distincts de la qualification 5W")
    variables = Edk2VarStore(str(magasin)).get_varlist()
    if variables["SecureBootEnable"].data != bytes([1]) or not variables["PK"].data:
        raise ValueError("Le banc exige Secure Boot actif et une PK installée")
    sortie.mkdir(parents=True, exist_ok=False)
    qemu = subprocess.check_output(["qemu-system-x86_64", "--version"], text=True).splitlines()[0]
    with banc.qualification(NOVA.parents[1], {}):
        # Même firmware et même topologie que la cellule active. Vérifier le RIP à chaque borne.
        bornes = tuple(ref["cas"]["actif-dmar"]["bornes"])
        row = banc.essai(sortie / "sans-console", fw, magasin, image, True,
                         bornes_prevues=bornes, temoin_console=sans_console)
    if row["journal"] != ["STANDARD REFUS 17 : console | arret local"] or not row["arret_cpu"]:
        raise ValueError("Refus ou arrêt terminal manquant dans la RAM")
    if any(row["trace"].values()):
        raise ValueError("Activité des périphériques pendant le refus")
    serie = (sortie / "sans-console/serie.log").read_bytes()
    if b"STANDARD REFUS" in serie:
        raise ValueError("Sortie série inattendue après perte de ConOut")
    registres = (sortie / "sans-console/registres.txt").read_text()
    flags = re.search(r"RFL=([0-9a-fA-F]+)", registres)
    if not flags or int(flags[1], 16) & 0x200:
        raise ValueError("Arrêt CPU sans preuve de IF=0")
    mesure = dict(qemu=qemu, ovmf=ref["ovmf"], image=ref["image"], magasin=banc.digest(magasin),
                  observation_apres_arret_ms=350, **row)
    (sortie / "mesure.json").write_text(json.dumps(mesure, indent=2) + "\n")
    print(row["journal"][0], "| HLT et compteurs vérifiés")


if __name__ == "__main__":
    main()
