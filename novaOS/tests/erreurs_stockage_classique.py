"""Vérifier les retours du client natif et les deux générations P9 par redémarrage QEMU."""
import argparse
from pathlib import Path
import re
import subprocess

from bloc import Bench
from fichiers import PATTERN, SOURCE
from preuves_stockage import (copie_reprise, dossier_essai, empreinte, enregistrer,
                             figer_initiale)

CAS = ("lecture", "ecriture", "f1", "ancre", "f2", "succes")


def faute(point, secteur_ancre):
    if point == "succes":
        return None
    evt = {"lecture": "read_aio", "ecriture": "write_aio", "ancre": "write_aio",
           "f1": "flush_to_disk", "f2": "flush_to_disk"}[point]
    # Les FLUSH éventuels de l'ouverture précèdent la première écriture CoW.
    etats = []
    if point in ("f1", "f2", "ancre"):
        etats.append("write_aio")
    if point in ("f2", "ancre"):
        etats.append("flush_to_disk")
    conf = "".join(f'[set-state]\nevent = "{evenement}"\nstate = "{i}"\nnew_state = "{i+1}"\n'
                   for i, evenement in enumerate(etats, 1))
    # Le firmware peut lire le disque avant le pilote NovaOS. Tenir la faute
    # pendant cette VM et retirer blkdebug au redémarrage sur copie.
    conf += f'[inject-error]\nevent = "{evt}"\nstate = "{len(etats)+1}"\nerrno = "5"\n'
    if point == "ancre":
        conf += f'sector = "{secteur_ancre}"\n'
    return conf


def inspecter(lecteur, image, secteur):
    def lire(op, *params):
        with image.open("rb") as flux:
            return subprocess.check_output([lecteur, op, "256", str(secteur), *params], stdin=flux)

    ancres = re.findall(r"slot=(\d+) génération=(\d+) blocs=256 FS=(\w+) LV=(\w+) manifeste=(\d+) sha256=([0-9a-f]{64})".encode(),
                        lire("inspect"))
    assert {int(r[0]) for r in ancres} == {0, 1} and len(ancres) == 2, ancres
    fs, lv = (b"17" * 16, b"70" * 16)
    assert all(r[2:4] == (fs, lv) for r in ancres)
    lire("verify", fs, lv)
    generations = {}
    for slot, gen, _, _, bloc, digest in ancres:
        generations[int(slot)] = dict(generation=int(gen), bloc=int(bloc), manifeste=digest.decode())
    tete = max(generations, key=lambda slot: generations[slot]["generation"])
    assert generations[tete]["generation"] == generations[1 - tete]["generation"] + 1
    assert lire("salvage", fs, lv, str(tete), "/source") == SOURCE
    sortie = lire("salvage", fs, lv, str(tete), "/sortie")
    assert sortie in (b"ancien", PATTERN)
    return tete, generations, sortie == PATTERN


def essai(args, point):
    with dossier_essai(args.rapport, args.arch, args.secteur, "classique-" + point,
                       conserver=True, parametres={"profil": "files-inject", "atime": "No"}) as dossier:
        banc = Bench(dossier, args.arch, args.secteur)
        banc.env["NOVA_KERNEL_ELF"] = str(args.noyau)
        subprocess.run([args.generateur, banc.image, str(args.secteur)], check=True)
        initiale = figer_initiale(banc.image)
        initial, avant, modifie = inspecter(args.lecteur, initiale, args.secteur)
        assert not modifie
        config = faute(point, (1 - initial) * 8)  # QEMU numérote en unités de 512 octets.
        fichier = None
        if config is not None:
            fichier = dossier / "injection.conf"
            fichier.write_text(config)
        serie = banc.run("files-inject", "sauvegarde", debug=fichier, diagnostic=True)
        code = {"succes": 0, "ancre": 22, "f2": 22}.get(point, 26)
        assert f"client=Exit({code})" in serie and "retrait=Ok(())" in serie, serie[-1500:]
        assert ("FICHIERS transaction confirmee" in serie) == (point == "succes")
        if config:
            op = {"lecture": 0, "ecriture": 1, "ancre": 1, "f1": 2, "f2": 2}[point]
            assert f"BLOC erreur peripherique op={op} statut=1" in serie
        image = banc.image
        banc.image = copie_reprise(image)
        tete, apres, nouveau = inspecter(args.lecteur, image, args.secteur)
        assert apres[initial] == avant[initial]
        assert apres[tete]["generation"] == avant[initial]["generation"] + int(nouveau)
        if point != "f2":
            assert nouveau == (point == "succes")
        retour = banc.run("files-recover", "reprise", readonly=True, diagnostic=True)
        etat = "nouveau" if nouveau else "ancien"
        assert f"FICHIERS oracle {etat}" in retour and "BLOC session terminee ok=true" in retour
        assert inspecter(args.lecteur, banc.image, args.secteur) == (tete, apres, nouveau)
        assert empreinte(banc.image) == empreinte(image)
        resultat = dict(point=point, secteur=args.secteur, arch=args.arch, statut_application=code,
                        source_octets=len(SOURCE), destination=etat, destination_octets=len(PATTERN if nouveau else b"ancien"),
                        ancres_avant=avant, ancres_apres=apres, initiale_sha256=empreinte(initiale),
                        arret_sha256=empreinte(image), reprise_sha256=empreinte(banc.image),
                        retrait_confirme=True, remontage_lecture_seule=True, dossier=dossier.name)
        enregistrer(dossier, "verification.json", resultat)
    return resultat


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64", "x86_64"))
    p.add_argument("secteur", choices=(512, 4096), type=int)
    p.add_argument("rapport", type=Path)
    for nom in ("noyau", "lecteur", "generateur"):
        p.add_argument("--" + nom, required=True, type=lambda s: Path(s).resolve(strict=True))
    p.add_argument("--point", choices=CAS)
    a = p.parse_args()
    a.rapport.parent.mkdir(parents=True, exist_ok=True)
    if a.rapport.exists():
        p.error("choisir un rapport neuf")
    resultats = [essai(a, point) for point in ((a.point,) if a.point else CAS)]
    enregistrer(a.rapport.parent, a.rapport.name, resultats)
