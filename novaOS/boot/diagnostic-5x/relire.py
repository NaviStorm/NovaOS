"""Relire les seules pièces du défaut ConOut sans démarrer une VM."""

import json
from pathlib import Path
import platform
import re
from zipfile import ZipFile

NOVA = Path(__file__).resolve().parents[2]
PREUVES = NOVA / "docs/validation/diagnostic-uefi-5x"


def verifier():
    ref = json.loads((NOVA / "docs/validation/diagnostic-uefi-5w/mesures.json").read_bytes())
    row = json.loads((PREUVES / "console.json").read_bytes())
    with ZipFile(PREUVES / "console-qemu.zip") as z:
        regs = z.read("registres.txt").decode("ascii")
        rip = int(re.search(r"\bRIP=([0-9a-fA-F]+)", regs)[1], 16)
        flags = int(re.search(r"\bRFL=([0-9a-fA-F]+)", regs)[1], 16)
        fin = row["bornes"][1]
        if "HLT=1" not in regs or flags & 0x200 or not fin <= rip < fin + 16:
            raise ValueError("Le CPU n'est pas au point d'arrêt local attendu")
        terminal = ["STANDARD REFUS 17 : console | arret local"]
        if z.read("journal.txt").decode("ascii").splitlines() != terminal or row["journal"] != terminal:
            raise ValueError("Refus terminal en RAM absent")
        if b"STANDARD" in z.read("serie.log") or z.read("transactions.log") or any(row["trace"].values()):
            raise ValueError("Sortie console ou événement de périphérique inattendu")
        captures = [json.loads(l) for l in z.read("compteurs-qmp.jsonl").splitlines()]
    if len(captures) != 3:
        raise ValueError("Trois instantanés QMP requis")
    for nom in ("interne", "externe"):
        compteurs = [next(v["stats"] for v in c if v.get("device") == nom) for c in captures]
        for cle in ("rd_bytes", "wr_bytes", "rd_operations", "wr_operations", "flush_operations"):
            if compteurs[1][cle] != compteurs[2][cle]:
                raise ValueError(f"E/S durant le diagnostic sur {nom}")
            if nom == "interne":
                for v, preuve in zip(compteurs, (row["reset"], row["entree"]["interne"], row["fin"]["interne"])):
                    if v[cle] != preuve[cle]:
                        raise ValueError(f"Compteur QMP interne incohérent ({cle})")
    if row["entree"] != row["fin"] or not row["arret_cpu"] or not row["disque_intact"]:
        raise ValueError("État de la machine aux bornes")
    origine = ref["cas"]["actif-dmar"]
    if (row["ovmf"], row["image"], row["bornes"], row["entree"]["variables"]) != (
        ref["ovmf"], ref["image"], origine["bornes"], origine["entree"]["variables"]
    ):
        raise ValueError("Écart avec l'image, le code OVMF, le magasin ou les bornes publiés")
    print(platform.machine(), "refus 17 en RAM, HLT local, IF=0, SATA et USB stables")


if __name__ == "__main__":
    verifier()
