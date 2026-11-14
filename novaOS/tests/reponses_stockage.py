"""Réponses IPC et retrait pendant une vraie sauvegarde ; un processus hôte par VM."""
import argparse
import os
from pathlib import Path
import re
import subprocess

import vi_volumes as volumes
from cli import Terminal
from preuves_stockage import copie_reprise, dossier_essai, empreinte, enregistrer, figer_initiale

MODES = {"tardive": 1, "annulation": 2, "ancien-tag": 3, "mort-relais": 4,
         "mort-pilote": 5, "abandon": 6, "revocation": 0}
TAILLE = 4 << 20


def verifier(image, secteur, ids, destination):
    def lire(op, *args, check=True):
        with image.open("rb") as source:
            return subprocess.run([volumes.HOST, op, str(volumes.BLOCKS), str(secteur), *args],
                                  stdin=source, capture_output=True, check=check)

    lire("verify", *ids)
    slots = re.findall(r"slot=(\d+) génération=(\d+)".encode(), lire("inspect").stdout)
    if len(slots) != 2 or len({int(g) for _, g in slots}) != 2:
        raise RuntimeError(f"Ancres absentes ou non distinctes : {slots!r}")
    slot = max(slots, key=lambda pair: int(pair[1]))[0]
    if lire("salvage", *ids, slot, "/f4194304").stdout != volumes.contents(TAILLE):
        raise RuntimeError("Source divergente")
    dest = lire("salvage", *ids, slot, "/" + destination, check=False)
    if dest.returncode == 0 or b"NotFound" not in dest.stderr:
        raise RuntimeError("La destination doit être absente avant F1")
    return {s.decode(): int(g) for s, g in slots}


def verifier_serie(serie, mode):
    if ("fichier enregistré" in serie or "erreur d’E/S" not in serie or
            re.findall(r"FS0170 plan resultat statut=(\d+)", serie) != ["26"] or
            re.findall(r"FS0170 delegation reponse op=30 statut=(\d+)", serie) != ["26"]):
        raise RuntimeError("La réponse applicative doit rester un refus d’E/S")
    transport = re.search(r"FS0170 transport (\w+) op=3 phase=4 n=(\d+) tag=(\d+) delai=(\d+) statut=(\d+)", serie)
    if not transport:
        raise RuntimeError("Première erreur du transport absente")
    etape, n, tag, delai, statut = transport.groups()
    tag, statut = int(tag), int(statut)
    reservation, admission_fermee = [], None
    preuves = re.findall(r"BLOC0170 tag=([a-f0-9]{16}) op=([a-f0-9]{16}) rc=([a-f0-9]{16})", serie)
    relations = [(int(t, 16), int(op, 16), int(rc, 16)) for t, op, rc in preuves]
    if mode:
        attendus = [(t, rc) for t, op, rc in relations if op == 200 + mode]
        if len(attendus) != 1 or attendus[0][0] != tag - int(mode == 3):
            raise RuntimeError(f"Tag ou injection inattendus : {relations!r}, tag {tag}")
        if mode == 3 and attendus[0][1] != 0:
            raise RuntimeError("Aucun envoi effectif de la réponse")
        if mode in (1, 2) and attendus[0][1] != 8:
            raise RuntimeError("Le receveur ayant abandonné doit laisser expirer l'envoi")
        if mode in (4, 5) and ((etape, statut) != ("reception", 11) or
                                attendus[0][1] != (37 if mode == 4 else 38) or
                                ("bloc=Exit(37)" if mode == 4 else "pilote=Exit(38)") not in serie):
            raise RuntimeError("Mort du processus non confirmée par le superviseur")
        if mode in (1, 6) and (etape, statut) != ("reception", 8):
            raise RuntimeError("Expiration attendue")
        if mode == 2 and (etape, statut) != ("reception", 9):
            raise RuntimeError("Réception réellement annulée attendue")
        if mode == 3 and (etape, statut) != ("identite", 3):
            raise RuntimeError("Rejet attendu de la réponse au tag précédent")
        if mode == 6:
            prefixe = "SURETE0170 reservation "
            lignes = [l.split(prefixe, 1)[1] for l in serie.splitlines() if prefixe in l]
            compte = re.findall(r"SURETE0170 apres_refus reservations=(\d+)$", serie, re.MULTILINE)
            if len(compte) != 1 or int(compte[0]) != len(lignes) or len(lignes) > 1:
                raise RuntimeError("Recensement superviseur incomplet ou ambigu")
            admission_fermee = True
            for ligne in lignes:
                champs = re.fullmatch(r"tag=(\d+) pages=(\d+) transfert=(true|false) "
                                      r"retrait=(true|false) doute=(true|false) progression=(\d+)", ligne)
                if not champs or tuple(map(int, champs.group(1, 2))) != (tag, int(n)):
                    raise RuntimeError("Réservation mal formée ou sans rapport avec le lot")
                if champs[3] != "false" or champs[6] != "0":
                    raise RuntimeError("Le relais retenu ne peut avoir déclenché de transfert")
                reservation.append(champs.groups())
                admission_fermee = champs[4] == "true" or champs[5] == "true"
    else:
        # Le noyau émet ce marqueur après assertions sur WOULD_BLOCK en vol,
        # le retrait après drainage, la révocation du descendant et le compteur de kicks.
        if (serie.count("COUPURE groupe retrait") != 1 or (etape, statut) != ("reponse", 12)
                or "retrait=Ok(())" not in serie):
            raise RuntimeError("Retrait après kick, refus du lot ou drainage non confirmé")
    return dict(transport=etape, tag=tag, pages=int(n), delai_ticks=int(delai),
                statut_transport=statut, sondes_ipc=relations, reservations_apres_refus=reservation,
                cloture_admission_observee=admission_fermee)


def essai(dossier, arch, secteur, scenario, *, attendre_retrait=False):
    mode = MODES[scenario]
    if mode:
        os.environ["NOVA_ESSAI_REPONSE"] = str(7 if attendre_retrait else mode)
    else:
        os.environ.pop("NOVA_ESSAI_REPONSE", None)
    volumes.SIZES, volumes.BLOCKS = (TAILLE,), 4096
    image, ids = volumes.prepare(dossier, secteur)
    initiale = figer_initiale(image)
    destination = "coupure-retrait" if scenario == "revocation" else "essai-reponse"
    ancres_neuves = verifier(initiale, secteur, ids, destination)
    journal = dossier / "sauvegarde.serial"
    with Terminal(arch, secteur, image, "reponse-" + scenario, test=True,
                  journal=journal, diagnostic=True) as vm:
        if attendre_retrait:
            # Témoigner du coût à froid des tables persistantes du noyau sur le même support.
            vm.until(b"nsh> ", 120)
            vm.vm.stdin.write(b"vi f4194304\n")
            vm.until(b"\x1b[6n")
            vm.vm.stdin.write(b"\x1b[24;80R")
            vm.until("f4194304 [enregistré]".encode())
            vm.vm.stdin.write(b"\x10:w echauffement\r")
            vm.until("Saisir OUI puis Entrée : ".encode())
            vm.vm.stdin.write(b"OUI\r")
            vm.until("fichier enregistré".encode(), 30, refus="erreur d’E/S".encode())
            vm.vm.stdin.write(b":q!\r")
            vm.until(b"nsh> ", 30)
            vm.vm.stdin.write(b"exit\n")
            vm.until(b"LOTS0170 temoin termine", 45)
        vm.until(b"nsh> ", 120)
        vm.vm.stdin.write(b"vi f4194304\n")
        vm.until(b"\x1b[6n")
        vm.vm.stdin.write(b"\x1b[24;80R")
        vm.until("f4194304 [enregistré]".encode())
        vm.vm.stdin.write(f"\x10:w {destination}\r".encode())
        vm.until("Saisir OUI puis Entrée : ".encode())
        # La lecture de la source par vi peut publier son atime. L'éditeur
        # attend la confirmation. Cette lecture indépendante de l'image brute
        # ne lance aucun appel d'écriture ni de synchronisation dans l'invité.
        avant_oui = verifier(image, secteur, ids, destination)
        vm.vm.stdin.write(b"OUI\r")
        vm.until("erreur d’E/S".encode(), 30, refus="fichier enregistré".encode())
        if mode in (1, 2, 3):
            # La trame de succès tardive a bien été envoyée (ou refusée à l'envoi).
            sonde = f"op={200 + mode:016x} rc=".encode()
            if sonde not in journal.read_bytes():
                vm.until(sonde, 15)
        vm.vm.stdin.write(b":q!\r")
        vm.until(b"nsh> ", 30)
        if mode != 6 or attendre_retrait:
            vm.vm.stdin.write(b"exit\n")
            vm.until(b"BLOC session terminee", 45)
    serie = journal.read_text(errors="replace")
    diagnostic = verifier_serie(serie.split("LOTS0170 temoin termine")[-1], mode)
    # Lire les octets avant et après un remontage sur une copie du disque arrêté.
    apres = copie_reprise(image)
    arrets = verifier(image, secteur, ids, destination)
    os.environ.pop("NOVA_ESSAI_REPONSE", None)
    with Terminal(arch, secteur, apres, "reprise-" + scenario, test=True,
                  journal=dossier / "reprise.serial", diagnostic=True) as vm:
        vm.until(b"nsh> ", 120)
        vm.exit()
    reprise = verifier(apres, secteur, ids, destination)
    if arrets != avant_oui or reprise != avant_oui:
        raise RuntimeError(f"Générations divergentes : {avant_oui}, {arrets}, {reprise}")
    rapport = dict(architecture=arch, secteur=secteur, scenario=scenario, injection=mode,
                   statut_public=26, reponse_declaree_durable=False,
                   source_exacte=True, destination_absente=True, ancres_creation=ancres_neuves,
                   ancres_avant_consentement=avant_oui, ancres=reprise,
                   image_initiale=empreinte(initiale), image_apres=empreinte(image),
                   copie_redemarree=empreinte(apres), series=empreinte(journal),
                   **diagnostic,
                   bilan=re.findall(r"BLOC bilan [^\r\n]+", serie))
    enregistrer(dossier, "observations.json", rapport)
    if diagnostic["cloture_admission_observee"] is False:
        raise RuntimeError("Après le refus à vi, le lot sans envoi au pilote reste "
                           "réservé avec retrait=false, doute=false")
    return rapport


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("architecture", choices=("aarch64", "x86_64"))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("scenario", choices=MODES)
    p.add_argument("rapport", type=Path, help="fichier neuf, un scénario par invocation")
    args = p.parse_args()
    if args.rapport.exists():
        p.error("Le rapport doit être neuf")
    with dossier_essai(args.rapport, args.architecture, args.secteur, args.scenario,
                      conserver=True, parametres=dict(octets=TAILLE, sonde=MODES[args.scenario])) as dossier:
        rapport = essai(dossier, args.architecture, args.secteur, args.scenario)
    enregistrer(args.rapport.resolve().parent, args.rapport.name, rapport)
