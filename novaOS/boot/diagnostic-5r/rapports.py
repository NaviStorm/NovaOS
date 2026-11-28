"""Préparer les rapports à partir des preuves QEMU vérifiées, sur fichiers ordinaires."""

import argparse
import json
import zipfile
from pathlib import Path

import qualification as qual

MOTIFS = {
    0: "Inventaire achevé",
    6: "Manifeste de pilote refusé",
    18: "Empreinte ou format du paquet ELF invalide",
    44: "Propriété de la surface GOP incohérente",
    50: "Contrat USB absent ou de longueur invalide",
    51: "Signature du contrat USB refusée par l’autorité attendue",
    52: "Époque ou incarnation incohérente",
    53: "Identité du PE incohérente",
    54: "Chemin USB incohérent",
    55: "Permissions ou mappings DMA incohérents",
    56: "Transactions USB non toutes achevées",
    57: "Contrat remis différent du contrat accepté par le BDS",
}
PREALABLES = {
    "clone": "Plusieurs supports USB identiques présents",
    "reconnexion": "Incarnation du support perdue",
    "quarantaine-vol": "Transaction xHCI observée en vol, restitution refusée",
    "substitue": "L’image chargée diffère du PE attendu",
    "hors-db": "OVMF refuse le PE dans le magasin 5O original",
}


def produire(archive, pe, destination):
    qual.verifier(archive, pe)
    with zipfile.ZipFile(archive) as source, zipfile.ZipFile(destination, "x") as out:
        for mode, cas in qual.cellules():
            prefix = f"{mode}-{cas}/"
            bilan = json.loads(source.read(prefix + "bilan.json"))
            entree = cas not in PREALABLES
            rapport = {
                "schema": "novaos.rapport-laboratoire/5r",
                "epoque_synthetique": bilan["epoque"],
                "profil": "QEMU Q35 x86_64",
                "secure_boot_banc": mode,
                "decision": bilan["decision"],
                "premier_efi_execute": entree,
                "origine_verdict": "premier EFI" if entree else "banc avant entrée EFI",
                "pe_observe_sha256": bilan["pe"],
                "media_sha256": bilan["image"],
                "compteurs_disque_interne": bilan["apres"]["interne"],
                "empreinte_stockage_apres": bilan["stockage_apres"],
                "activation_materielle_autorisee": False,
                "restitution_individuelle_autorisee": False,
                "export_usb_natif_autorise": False,
            }
            if entree:
                ram = json.loads(source.read(prefix + "rapport.json"))
                grant, fields = qual.champs(source.read(prefix + "livraison.bin"))
                rapport.update(
                    diagnostic=ram,
                    motif=MOTIFS[ram["statut"]],
                    contrat_remis_sha256=qual.empreinte(fields[0])
                    if fields[0]
                    else None,
                    bail_graphique_sha256=qual.empreinte(grant),
                    affichage_valide=ram["statut"] != 44,
                    tables_vtd_sha256=qual.empreinte(
                        source.read(prefix + "entree.bin")
                    ),
                    activite_apic=json.loads(source.read(prefix + "verdict-apic.json")),
                )
            else:
                rapport.update(motif=PREALABLES[cas], affichage_valide=False)
            contenu = (
                json.dumps(rapport, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
            )
            texte = f"NovaOS 5R — {rapport['motif']}\nSource du verdict : {rapport['origine_verdict']}. Secure Boot du banc : {mode}.\n"
            if entree:
                texte += source.read(prefix + "journal.txt").decode()
            texte += (
                "Disque interne synthétique : zéro E/S. Aucune activation matérielle.\n"
            )
            for nom, data in (("rapport.json", contenu), ("journal.txt", texte)):
                item = zipfile.ZipInfo(prefix + nom, (2000, 1, 1, 0, 0, 0))
                item.external_attr = 0o100644 << 16
                out.writestr(item, data.encode(), zipfile.ZIP_DEFLATED, compresslevel=9)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for n in ("observations", "pe", "sortie"):
        p.add_argument(n, type=Path)
    args = p.parse_args()
    produire(args.observations, args.pe, args.sortie)
