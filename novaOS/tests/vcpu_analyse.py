"""Post-traitement hors ligne des sauvegardes UTM mesurées avec --profil-charge."""
import argparse
import json
import os
from collections import defaultdict
from pathlib import Path


def percentiles(valeurs):
    serie = sorted(valeurs)
    if not serie:
        return None
    return {"n": len(serie), **{f"p{p}": serie[(p*len(serie)+99)//100-1] for p in (50, 95, 99)},
            "min": serie[0], "max": serie[-1]}


def charge_sur_intervalle(charge, debut, fin):
    avant, apres = charge["avant"], charge["apres"]
    ticks = [(b-a) % (1 << 32) for a, b in zip(avant["cpu_ticks"], apres["cpu_ticks"], strict=True)]
    if len(ticks) != 4 or not avant["monotone_ns"] <= debut <= fin <= apres["monotone_ns"] - apres["cout_ns"]:
        raise ValueError("Les instantanés hôte doivent encadrer la commande")
    if avant["taille_page"] != apres["taille_page"]:
        raise ValueError("Taille de page hôte incohérente")
    return {
        "delta_ticks_utilisateur_systeme_inactif_nice": ticks,
        "cpu_global_actif_pct": round((sum(ticks)-ticks[2])*100/sum(ticks), 3) if sum(ticks) else None,
        "fenetre_instantanes_ns": apres["monotone_ns"]-avant["monotone_ns"],
        "avant_commande_ns": debut-avant["monotone_ns"], "apres_commande_ns": apres["monotone_ns"]-fin,
        "pression_avant_apres": [avant["pression"], apres["pression"]],
        "delta_evenements_vm": {k: apres["pages"][k]-avant["pages"][k] for k in
                                ("Pageins", "Pageouts", "Swapins", "Swapouts", "Compressions", "Decompressions")},
        "disk0_delta_transferts": apres["disque_transferts"]-avant["disque_transferts"],
        "disk0_delta_mio": round(apres["disque_mio"]-avant["disque_mio"], 2),
    }


def analyse_thread(vcpu, frontieres):
    origine = vcpu["origine_monotone_ns"]
    echantillons = vcpu["echantillons"]
    # La sonde horodate au milieu de l'appel libproc, arrondi à l'entier inférieur.
    appels = [(origine+h-d//2, origine+h+(d+1)//2) for h, _, _, _, d in echantillons]
    cumuls = [u+s for _, u, s, _, _ in echantillons]
    if not cumuls or any(a>b for a,b in zip(cumuls, cumuls[1:])):
        raise ValueError("Compteur CPU non monotone ou vide")
    if cumuls[-1] != vcpu["utilisateur_ns"] + vcpu["systeme_ns"]:
        raise ValueError("Dernier échantillon CPU incohérent")
    bornes = []
    for t in frontieres:
        i = next((k for k in reversed(range(len(appels))) if appels[k][1] <= t), None)
        j = next((k for k in range(len(appels)) if appels[k][0] >= t), None)
        if i is None or j is None:
            raise ValueError("Frontière de commande non encadrée par deux appels libproc")
        bornes.append([i, j])

    phases = {}
    for nom, a, b in (("avant_oui", 0, 1), ("apres_oui", 1, 2), ("commande", 0, 2)):
        i, j = bornes[a]
        k, m = bornes[b]
        # Les bornes s'appliquent aux compteurs publiés. Une mise à jour
        # différée peut donner plus de CPU que de temps mural sur un petit intervalle.
        cpu_min = max(0, cumuls[k]-cumuls[j])
        cpu_max = cumuls[m]-cumuls[i]
        duree = frontieres[b]-frontieres[a]
        phases[nom] = {"ecoule_ns": duree, "cpu_comptabilise_ns": [cpu_min, cpu_max],
                       "ecoule_moins_cpu_ns": [duree-cpu_max, duree-cpu_min]}
    intervalles = []
    for i in range(bornes[0][1], bornes[-1][0]):
        dt = echantillons[i+1][0]-echantillons[i][0]
        dc = cumuls[i+1]-cumuls[i]
        intervalles.append({"indices": [i, i+1], "duree_ns": dt, "cpu_comptabilise_ns": dc,
                            "ecoule_moins_cpu_ns": dt-dc})
    return {"indices_encadrant_commande_oui_succes": bornes, "phases": phases,
            "plus_grand_ecart_ecoule_moins_cpu": max(intervalles, default=None, key=lambda x: x["ecoule_moins_cpu_ns"]),
            "plus_long_intervalle_observateur": max(intervalles, default=None, key=lambda x: x["duree_ns"]),
            "intervalles_de_plus_de_deux_periodes": [x["indices"][0] for x in intervalles
                                                     if x["duree_ns"] > 2*vcpu["periode_demandee_ns"]],
            "intervalles_cpu_superieur_a_ecoule": sum(x["ecoule_moins_cpu_ns"] < 0 for x in intervalles)}


def analyser(mesure):
    if mesure.get("repetitions"):
        raise ValueError("Une trace vCPU ne couvre qu'une sauvegarde")
    debut, oui, fin = mesure["commande_monotone_ns"]
    if not debut <= oui <= fin or not fin > debut:
        raise ValueError("Horodatages de commande désordonnés")
    for cle, valeur in (("sauvegarde_ms", fin-debut), ("apres_oui_ms", fin-oui)):
        if round(valeur/1e6, 3) != mesure[cle]:
            raise ValueError(f"Écart entre horloge et {cle}")
    if mesure["sonde"] != (mesure["compteur"] is not None):
        raise ValueError("Contradiction entre sonde invitée et résultat de comptage")
    return {"commande_ns": fin-debut, "apres_oui_ns": fin-oui, "seuil_atteint_ou_depasse": fin-debut >= 10**9,
            "thread": analyse_thread(mesure["vcpu"], [debut, oui, fin]),
            "charge": charge_sur_intervalle(mesure["charge_hote"], debut, fin)}


def fichiers(entrees):
    trouves = set()
    journaux_sans_resultat = set()
    for entree in entrees:
        if entree.is_file():
            groupe = [entree]
        elif (entree/"resultats.json").is_file():
            groupe = [entree/"resultats.json"]
        else:
            groupe = sorted(entree.glob("*/resultats.json"))
            for log in entree.glob("*.log"):
                if not (log.with_suffix("")/"resultats.json").is_file():
                    journaux_sans_resultat.add(log.resolve())
        if not groupe:
            raise ValueError(f"Aucun résultat de sauvegarde dans {entree}")
        trouves.update(x.resolve() for x in groupe)
    return sorted(trouves), sorted(journaux_sans_resultat)


def rapport(entrees):
    chemins, non_termines = fichiers(entrees)
    racine = Path(os.path.commonpath([p.parent.parent for p in chemins]))
    essais = []
    groupes = defaultdict(list)
    depassements = {"avec_sonde_invitee": [], "sans_sonde_invitee": []}
    cles = ("profil", "cas", "secteur", "preallocation", "sonde", "environnement",
            "sortie_arriere_plan", "traces_bloc", "index_parcouru", "politique_demandee")
    for chemin in chemins:
        try:
            brut = json.loads(chemin.read_text())
            analyse = analyser(brut)
        except (KeyError, ValueError, TypeError) as exc:
            raise ValueError(f"Résultat inexploitable dans {chemin} ({exc})") from exc
        politique = brut.get("politique_demandee", "-B" if brut["sortie_arriere_plan"] else "ordinaire")
        if politique not in ("ordinaire", "-b", "-B") or brut["sortie_arriere_plan"] != (politique == "-B"):
            raise ValueError(f"Politique d’arrière-plan incohérente dans {chemin}")
        cellule = dict(profil=brut["charge_hote"]["profil"], **{k: brut[k] for k in cles[1:-1]},
                       politique_demandee=politique)
        ident = str(chemin.parent.relative_to(racine))
        if chemin.name != "resultats.json":
            ident += "/" + chemin.name
        essai = {"essai": ident, "analyse": analyse, "releves": brut}
        groupes[tuple(cellule.values())].append(essai)
        essais.append(essai)
        if analyse["seuil_atteint_ou_depasse"]:
            depassements["avec_sonde_invitee" if brut["sonde"] else "sans_sonde_invitee"].append(
                dict(essai=ident, commande_ns=analyse["commande_ns"], **cellule))
    cellules = []
    for cle, serie in sorted(groupes.items()):
        analyses = [essai["analyse"] for essai in serie]
        releves = [essai["releves"] for essai in serie]
        phases = {}
        for nom in ("commande", "avant_oui", "apres_oui"):
            phases[nom] = {
                champ: {borne: percentiles([a["thread"]["phases"][nom][champ][i] for a in analyses])
                        for i, borne in enumerate(("inferieure", "superieure"))}
                for champ in ("cpu_comptabilise_ns", "ecoule_moins_cpu_ns")}
        cellule = {**dict(zip(cles, cle)), "essais": [v["essai"] for v in serie],
                   "commande_ns": percentiles([a["commande_ns"] for a in analyses]),
                   "apres_oui_ns": percentiles([a["apres_oui_ns"] for a in analyses]),
                   "nombre_au_moins_1s": sum(a["seuil_atteint_ou_depasse"] for a in analyses),
                   "phases_darwin": phases,
                   "cpu_global_actif_pct": percentiles([a["charge"]["cpu_global_actif_pct"] for a in analyses
                                                         if a["charge"]["cpu_global_actif_pct"] is not None]),
                   "backend_hmp": {k: percentiles([r["backend_hmp"][k] for r in releves])
                                   for k in ("rd_total_time_ns", "wr_total_time_ns", "flush_total_time_ns")}}
        if cellule["sonde"]:
            cellule["ipc_avec_controles_locaux"] = percentiles([r["compteur"]["total_avec_controles_locaux"]
                                                              for r in releves])
            for i, barriere in enumerate(("F1_ms", "F2_ms")):
                cellule[barriere] = percentiles([r["compteur"]["flush_physiques_ms"][i] for r in releves])
        cellules.append(cellule)
    for liste in depassements.values():
        liste.sort(key=lambda v: v["commande_ns"], reverse=True)
    return {"methode": {"percentiles": "Rang supérieur ceil(p*n), par cellule. Suffixes _ns et _ms pour les durées.",
                        "critere": "Temps de :w au succès strictement inférieur à 1 s.",
                        "cpu": "Somme utilisateur et système publiée par libproc, à la microseconde par compteur. "
                               "Les indices encadrent chaque frontière en incluant la durée de l'appel. "
                               "Les écarts mural moins CPU incluent attente et désordonnancement. "
                               "Leur signe peut dépendre de la mise à jour des compteurs.",
                        "charge": "Ticks hôte globaux modulo 2^32, activité indisponible pour un delta nul. "
                                  "Instantanés plus larges que la commande, iostat disk0 arrondi à 0,01 Mio."},
            "journaux_sans_resultat": [os.path.relpath(p, racine) for p in non_termines],
            "cellules": cellules, "seuil_atteint_ou_depasse": depassements, "essais": essais}


if __name__ == "__main__":
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("--sortie", type=Path, required=True)
    args.add_argument("entrees", nargs="+", type=Path)
    options = args.parse_args()
    sources, _ = fichiers(options.entrees)
    if options.sortie.resolve() in sources:
        args.error("La sortie doit être distincte des résultats bruts")
    resultat = rapport(options.entrees)
    options.sortie.write_text(json.dumps(resultat, ensure_ascii=False, separators=(",", ":"), allow_nan=False)+"\n")
