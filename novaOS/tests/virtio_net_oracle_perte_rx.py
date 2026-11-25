"""Vérifie hors ligne deux exécutions QEMU, nominale puis privée d'une réponse Ethernet."""
import argparse
import hashlib
import json
from pathlib import Path

from virtio_net_pair import trame


def lire(dossier, nom):
    return json.loads((dossier / nom).read_text())


def exiger(condition, message):
    if not condition:
        raise ValueError(message)


def verifier(dossier, perte):
    pair = lire(dossier, "pair.json")
    modele = lire(dossier, "modele-rx.json")
    campagne = lire(dossier, "campagne.json")
    serie = (dossier / "serie.log").read_text()
    elf = campagne.get("elf_sha256") or lire(dossier, "entrees.json")["novaos.elf"]
    exiger(pair["erreur"] is None and pair.get("perte", False) is perte, "Configuration ou erreur du pair")
    exiger(pair["succes"] is (not perte), "Issue du pair incohérente")
    # Le témoin nominal antérieur au mode de perte atteste ses 16 RX dans l’invité.
    if perte or "reponses_envoyees" in pair:
        exiger(pair["reponses_envoyees"] == 16 - int(perte), "Nombre de réponses incorrect")
    exiger(len(pair["trames"]) == 16, "Le NIC doit avoir émis seize trames")
    for i, observation in enumerate(pair["trames"]):
        octets = trame(i, False)
        exiger(observation["sequence"] == i and observation["octets"] == len(octets),
               "Séquence ou longueur TX incohérente")
        exiger(hashlib.sha256(octets).hexdigest() == observation["tx_sha256"],
               "Empreinte de la trame émise différente")
    exiger(lire(dossier, "fin.json")["pci_command"] & 4 == 0, "Bus Master encore actif")
    exiger("NET L2 saturation TX=16 statut=7" in serie, "File TX pleine non démontrée")
    complet = "NET L2 completions TX=16 RX=16 identifiants_uniques=true"
    code = 8 if perte else 0
    exiger(f"NET L2 pilote fin=Exit({code}) racines=0 ipc_avant=0 ipc_apres=0" in serie,
           "Issue du pilote ou restitution IPC incorrecte")
    exiger((complet in serie) is (not perte), "Complétions incompatibles avec le scénario")
    if perte:
        exiger("NET L2 refus seq=34 tag=3 statut=10" in serie, "Refus de la collecte absent")
        exiger("NET ARM bilan Err(Quarantine)" in serie or "NET Q35 L2 quarantaine=Quarantine" in serie,
               "Quarantaine de l'architecture absente")
    else:
        exiger(campagne.get("succes") is True or campagne.get("bilan") is True,
               "Exécution nominale incomplète")
    return {"elf": elf, "modele": modele["sha256"], "correctif": modele["correctif"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("nominal", type=Path)
    parser.add_argument("perte", type=Path)
    args = parser.parse_args()
    reference = verifier(args.nominal, False)
    exiger(verifier(args.perte, True) == reference, "ELF ou modèle distinct du témoin nominal")
    print(json.dumps({"controle_reussi": True, **reference}, indent=2))
