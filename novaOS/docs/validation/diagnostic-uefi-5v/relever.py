"""Relevé statique du seul paquet public MSI E16J5IMS.11D avec EDK II FMMT."""

import sys
from collections import Counter
from hashlib import sha256
from io import BytesIO
from pathlib import Path
from uuid import UUID
from zipfile import ZipFile

from core.FMMTParser import FMMTParser, ROOT_TREE


ZIP_SHA256 = "31707a5fa8f7b98c75832d992c5a159644b4522e3d175cfad0fd8f2343c0d178"
ZIP_TAILLE = 5_015_883
DOSSIER = "E16J5IMS.11D/"
GUIDS = {
    "EDKII_IOMMU_PPI": "70b0af26-f847-4bb6-aab9-cde84fc61431",
    "EDKII_IOMMU_PROTOCOL": "4e939de9-d948-4b0f-88ed-e6e1ce517c1e",
    "EFI_PCI_IO_PROTOCOL": "4cf5b200-68b8-4ca5-9eec-b23e3f50029a",
    "EFI_LOADED_IMAGE_PROTOCOL": "5b1b31a1-9562-11d2-8e3f-00a0c969723b",
}
ETIQUETTES = (
    "BootGuardPei", "BootGuardDxe", "VerifyFwBootGuard", "SaInit", "Tcg2Dxe",
    "Ahci", "AhciSmm", "AhciRecovery", "UsbRt",
)
# PE32, TE, expressions de dépendances DXE, PEI et MM.
SECTIONS = {0x10, 0x12, 0x13, 0x1B, 0x1C}


def relever(chemin):
    with chemin.open("rb") as source:
        archive = source.read(ZIP_TAILLE + 1)
    if len(archive) != ZIP_TAILLE or sha256(archive).hexdigest() != ZIP_SHA256:
        raise SystemExit("Refus : archive différente de la publication MSI attendue.")
    with ZipFile(BytesIO(archive)) as paquet:
        bios = paquet.read(DOSSIER + "E16J5IMS.11D")
        notice = paquet.read(DOSSIER + "[General Version] BIOS Update Instruction (BSU) v2.6_All.pdf")
    for nom, contenu in (("ZIP", archive), ("BIOS", bios), ("PDF", notice)):
        print(nom, len(contenu), sha256(contenu).hexdigest())

    analyse = FMMTParser("publication.fd", ROOT_TREE)
    analyse.ParserFromRoot(analyse.WholeFvTree, bios)
    a_visiter = [analyse.WholeFvTree]
    etiquettes = Counter()
    correspondances = Counter()
    motifs = {nom: UUID(guid).bytes_le for nom, guid in GUIDS.items()}
    fichiers = sections = 0
    while a_visiter:
        noeud = a_visiter.pop()
        a_visiter.extend(noeud.Child)
        if noeud.type == "FFS":
            fichiers += 1
            if noeud.Data.UiName:
                etiquettes[str(noeud.Data.UiName)] += 1
        if noeud.type != "SECTION" or noeud.Child or noeud.Data.Type not in SECTIONS:
            continue
        sections += 1
        for nom, motif in motifs.items():
            if motif in noeud.Data.Data:
                correspondances[nom] += 1
    print("FFS", fichiers, "avec_nom", sum(etiquettes.values()), "sections_examinees", sections)
    for nom in ETIQUETTES:
        print("etiquette", nom, etiquettes[nom])
    for nom in motifs:
        print("sections_avec_GUID", nom, correspondances[nom])


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage : relever.py E16J5IMS.11D.zip")
    relever(Path(sys.argv[1]))
