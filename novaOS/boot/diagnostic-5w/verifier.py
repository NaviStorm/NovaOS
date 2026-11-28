"""Contre-lecture hors VM de la livraison STANDARD, des traces et du canari."""

import gzip
from hashlib import sha256
import json
from pathlib import Path
import platform
import re
import sys
import tempfile
from zipfile import ZipFile

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5o"))
from image5o import verifier

PREUVES = NOVA / "docs/validation/diagnostic-uefi-5w"
ATTENDUS = {
    "actif-dmar": [0],
    "inactif-sans-dmar": [],
    "inactif-tes-actif": [1],
    "inactif-refus": [0],
    "actif-refus-firmware": None,
}


def exiger(condition, raison):
    if not condition:
        raise ValueError(raison)


def controler():
    r = json.loads((PREUVES / "mesures.json").read_bytes())
    exiger(ATTENDUS.keys() == r["cas"].keys(), "Cellule manquante ou inattendue")
    pe = gzip.decompress((PREUVES / "standard.efi.gz").read_bytes())
    exiger(
        sha256(pe).hexdigest() == (PREUVES / "standard.sha256").read_text().split()[0],
        "Identité du PE",
    )
    image = gzip.decompress((PREUVES / "novaos-standard-68mio.img.gz").read_bytes())
    exiger(sha256(image).hexdigest() == r["image"], "Identité du disque")
    build = NOVA / "build/5w"
    build.mkdir(exist_ok=True, parents=True)
    with tempfile.TemporaryFile(dir=build) as src:
        src.write(image)
        src.flush()
        verifier(src, 68 << 20, pe)
    with ZipFile(PREUVES / "observations.zip") as z:
        for nom, attendu in ATTENDUS.items():
            row = r["cas"][nom]
            exiger(row["disque_intact"], nom + " canari")
            serie = z.read(nom + "/serie.log")
            captures = [
                json.loads(l) for l in z.read(nom + "/compteurs-qmp.jsonl").splitlines()
            ]
            sata = [
                next(x["stats"] for x in c if x.get("device") == "interne")
                for c in captures
            ]
            if attendu is None:
                exiger(
                    row["repli_sata"]
                    and row["aucune_option"]
                    and row["refus_ovmf"]
                    and b"Access Denied" in serie
                    and b"Sata(" in serie
                    and b"No bootable option" in serie
                    and b"STANDARD" not in serie,
                    nom,
                )
                exiger(
                    sata[-1]["rd_operations"] == row["apres"]["rd_operations"] > 0
                    and sata[-1]["wr_operations"] == 0,
                    nom + " compteurs",
                )
                continue
            exiger(row["entree"] == row["fin"] and row["arret_cpu"], nom + " bornes")
            exiger(
                all(
                    c[k] == v
                    for c in sata[1:]
                    for k, v in row["entree"]["interne"].items()
                ),
                nom + " SATA",
            )
            hw = row["entree"]["materiel"]
            exiger(
                [u["gsts"] >> 31 for u in hw["unites"].values()] == attendu,
                nom + " TES",
            )
            journal = z.read(nom + "/journal.txt").decode("ascii").splitlines()
            exiger(
                journal == row["journal"]
                and "STANDARD" in journal[0]
                and "Protection DMA avant chargement : INDETERMINEE" in journal,
                nom + " confiance",
            )
            mode = nom.split("-", 1)[0]
            exiger(
                any("Secure Boot " + mode in l for l in journal), nom + " Secure Boot"
            )
            trace = z.read(nom + "/transactions.log").decode("ascii").splitlines()
            compteurs = {
                e: sum(l.startswith(e + " ") for l in trace) for e in row["trace"]
            }
            exiger(
                compteurs == row["trace"]
                and all(n == 0 for e, n in compteurs.items() if e != "apm_io_write"),
                nom + " traces",
            )
            if nom == "inactif-refus":
                exiger(
                    any("REFUS 6" in l for l in journal)
                    and not any(l.startswith("PCI ") for l in journal),
                    nom,
                )
                continue
            releve = {}
            for ligne in journal:
                m = re.fullmatch(
                    r"PCI (\S+) id=([\da-f]+) classe=([\da-f]+) BME=([01]) DRHD=\S+ pilote=(\S+)",
                    ligne,
                )
                if m:
                    exiger(m[1] not in releve, "RID dupliqué")
                    releve[m[1]] = (int(m[2], 16), int(m[3], 16), int(m[4]))
            externe = {
                rid: (d["ident"], d["classe_revision"] >> 8, (d["cmd"] >> 2) & 1)
                for rid, d in hw["pci"].items()
            }
            exiger(
                releve == externe
                and any("USB declare par UEFI" in l for l in journal)
                and any("activations 0" in l for l in journal)
                and "Inventaire termine" in journal[-1],
                nom + " PCI",
            )
            if attendu == [1]:
                s = row["stimulus"]
                exiger(
                    s["avant"]["gsts"] == s["avant"]["rtaddr"] == 0
                    and s["apres"]["rtaddr"] == s["racine_sans_entree"]
                    and s["apres"]["gsts"] >> 31 == 1,
                    "Origine du témoin actif",
                )
    print(
        platform.machine(),
        "5 cellules STANDARD vérifiées, PCI, traces, compteurs SATA, image GPT/FAT32 et PE concordants",
    )


if __name__ == "__main__":
    controler()
