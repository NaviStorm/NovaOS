"""Qualification de l'export diagnostic-stockage avec les sondes réponses puis endurance."""
import argparse
from pathlib import Path
import re
import sys

from bloc import Bench
from preuves_stockage import dossier_essai, empreinte, enregistrer, figer_initiale
from reponses_stockage import essai as essai_vi


def endurance(arch, secteur, dossier):
    banc = Bench(dossier, arch, secteur)
    with banc.image.open("xb") as image:
        image.write(b"NovaOS-test-DMA!")
        image.truncate(4 << 20)
    temoin = figer_initiale(banc.image)
    s = banc.run("ipc-lots-duree", cut="LOTS0170 fin abandons=1000", readonly=True,
                 timeout=3600, diagnostic=True)
    lignes = re.findall(r"LOTS0170 etape=(\w+) libres=(\d+) objets=(\d+) handles=(\d+) lots=(\d+) "
                        r"pages_logiques=(\d+) attente=(\d+) epinglees=(\d+) bail=(\d+) "
                        r"quarantaine=(false|true) generation=(\d+)", s)
    froid, initial, *epochs = lignes
    if (froid[0], initial[0]) != ("froid", "avant") or len(epochs) != 24:
        raise ValueError("Mesures de ressources incomplètes")
    if s.count("LOTS0170 endpoint_independant expiration=8 lot_conserve=256 ferme=0 OK") != 1:
        raise ValueError("Isolation de l’attente indépendante non vérifiée")
    generations = []
    for i in range(0, len(epochs), 3):
        avant, apres, fin = epochs[i:i+3]
        if ((avant[0], apres[0], fin[0]) != ("service_chaud", "service_apres", "fin_serie") or
                avant[1:-1] != apres[1:-1] or initial[1:-1] != fin[1:-1] or
                int(avant[-1]) >= int(fin[-1]) or int(avant[8]) <= i // 3):
            raise ValueError(f"Ressources divergentes : {avant}, {apres}, {fin}")
        generations.append(int(avant[8]))
    if initial[2:10] != ("0", "0", "0", "0", "0", "9" if arch == "x86_64" else "0", "0", "false"):
        raise ValueError(f"Le contrôle initial n’est pas au repos : {initial}")
    nombre = [int(n) for n in re.findall(r"LOTS0170 serie=\d+ abandons=(\d+) succes=17", s)]
    if nombre != [125] * 8:
        raise ValueError(f"Séries incomplètes : {nombre}")
    fin = re.search(r"LOTS0170 fin abandons=1000 succes=136 series=8 preemptions=(\d+) "
                    r"restitutions=1000 transition_max_cycles=(\d+) retrait_max_cycles=(\d+) hz=(\d+) CPU=(\d+) OK", s)
    if not fin:
        raise ValueError("Durée, préemptions ou bilan final absent")
    preemptions, transition, maximum, hz, cpu = map(int, fin.groups())
    if not (preemptions and transition and maximum and hz) or cpu != 4:
        raise ValueError("La campagne exige quatre vCPU et une préemption observée")
    if empreinte(banc.image) != empreinte(temoin):
        raise ValueError("Le support de lectures a changé")
    return dict(endpoint_independant_verifie=True, abandons=sum(nombre), lectures_confirmees=136, acteurs_recrees=24,
                snapshots=[dict(zip(("etape","cadres_libres","endpoints","handles","reservations",
                                     "pages_logiques","attentes","cadres_dma","bail","quarantaine",
                                     "generation_IPC"), ligne)) for ligne in lignes],
                epoques_du_pilote=generations, preemptions_moteur=preemptions,
                restitution_cycles_max=transition, restitution_us_max=transition * 1e6 / hz,
                retour_ipc_close_cycles_max=maximum, frequence_hz=hz, vcpu=cpu,
                retour_ipc_close_us_max=maximum * 1e6 / hz, disque_sha256=empreinte(temoin))


def fermeture_vi(arch, secteur, dossier):
    resultat = essai_vi(dossier, arch, secteur, "abandon", attendre_retrait=True)
    serie = (dossier / "sauvegarde.serial").read_text(errors="replace")
    temoin, serie = serie.split("LOTS0170 temoin termine")
    cadres = r"BLOC memoire cadres_init=(\d+) cadres_fin=(\d+) .*?buddy_init=(\d+) buddy_fin=(\d+)"
    froid = re.search(cadres, temoin)
    if not froid or "fichier enregistré" not in temoin or "client=Exit(0) retrait=Ok(())" not in temoin:
        raise ValueError("Témoin positif vi à froid incomplet")
    mesure = r"bail_session=(\d+) quarantaine=(true|false) epinglees=(\d+) kicks=(\d+) en_vol=(false)"
    avant = re.findall("LOTS0170 retenu " + mesure, serie)
    apres = re.findall("LOTS0170 refus lots=0 pages_logiques=0 " + mesure, serie)
    if len(avant) != 1 or avant != apres or avant[0][1] != "false" or int(avant[0][2]) == 0:
        raise ValueError(f"Le lot abandonné a modifié les ressources DMA ou soumis une commande : {avant} {apres}")
    cloture = re.findall(r"LOTS0170 fin_session bail=(\d+) quarantaine=(true|false) epinglees=(\d+)", serie)
    systeme = "9" if arch == "x86_64" else "0"  # Racine, domaine et file VT-d par machine.
    if cloture != [("0", "false", systeme)] or "retrait=Ok(())" not in serie:
        raise ValueError(f"Restitution matérielle en fin de session non confirmée : {cloture}")
    bilan = re.search(cadres, serie)
    if (not bilan or (bilan[1], bilan[3]) != (bilan[2], bilan[4]) or
            (froid[2], froid[4]) != (bilan[1], bilan[3])):
        raise ValueError("Bilan mémoire après destruction des services divergent")
    return dict(**resultat, cadres_froids_avant=int(froid[1]), cadres_froids_apres=int(froid[2]),
                cadres_libres_avant=int(bilan[1]),cadres_libres_apres=int(bilan[2]),
                session_dma_avant_et_apres_refus=int(avant[0][0]), cadres_dma_machine_et_session_au_refus=int(avant[0][2]),
                kicks_avant_et_apres_refus=int(avant[0][3]), baux_actifs_fin=0, cadres_dma_session_fin=0, cadres_dma_machine_fin=int(systeme))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("architecture", choices=("aarch64", "x86_64"))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("essai", choices=("vi", "endurance"))
    p.add_argument("rapport", type=Path)
    args = p.parse_args()
    if args.rapport.exists():
        sys.exit("Un chemin de rapport neuf est requis")
    args.rapport.parent.mkdir(parents=True, exist_ok=True)
    with dossier_essai(args.rapport, args.architecture, args.secteur, args.essai, conserver=True,
                      parametres=dict(serie="reservations-0170", scenario=args.essai)) as d:
        run = fermeture_vi if args.essai == "vi" else endurance
        ligne = run(args.architecture, args.secteur, d)
        ligne.update(architecture=args.architecture, secteur=args.secteur, dossier=d.name)
        enregistrer(d, "controle-final.json", ligne)
    enregistrer(args.rapport.resolve().parent, args.rapport.name, ligne)
