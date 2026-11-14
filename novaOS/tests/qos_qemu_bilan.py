"""Consolider les résultats QoS de QEMU direct et les différences au témoin de chaque bloc."""
import argparse
from collections import Counter, defaultdict
import json
from math import ceil
from pathlib import Path
from qos_qemu import ARCHS, CAS, POLITIQUES


def distribution(valeurs):
    valeurs = sorted(valeurs)
    if not valeurs:
        return {}
    def rang(p):
        return round(valeurs[ceil(p*len(valeurs))-1], 3)
    return dict(n=len(valeurs), min=round(valeurs[0], 3), p50=rang(.50), p95=rang(.95), p99=rang(.99),
                max=round(valeurs[-1], 3))


def reduire(r):
    commun = {k: r[k] for k in ("ordre", "repetition", "architecture", "secteur", "cas", "politique",
                                 "debut", "fin", "charge_avant", "charge_apres")}
    if "erreur" in r:
        return dict(commun, erreur=r["erreur"])
    m, q = r["mesure"], r["qos"]
    c = m["compteur"]
    if "attente_irq" in c:
        raise ValueError("Profil IRQ inattendu dans la qualification du polling")
    points = q["points"]
    debut = points[2]
    # Schéma tabulaire conservant chaque relevé et l’ordre temporel en réduisant les répétitions.
    samples = [[p[k] for k in ("type", "indice", "coeur", "qos_demandee", "relative", "priorite_base",
                               "priorite_courante")] + [p["cpu_ns"]-debut["cpu_ns"], p["ecoule_ns"]-debut["ecoule_ns"]]
               for p in points]
    return dict(commun, sha256_elf=r["sha256_elf"], sha256_serie=r["sha256_serie"],
                commande_ms=m["sauvegarde_ms"], avant_oui_ms=m["sauvegarde_ms"]-c["oui_au_succes_ms"],
                apres_oui_ms=c["oui_au_succes_ms"], cpu_vcpu_ms=q["publication"]["cpu_ms"],
                publication_ms=q["publication"]["ecoule_ms"], bornes_F1=q["F1"], bornes_F2=q["F2"],
                flush_physiques_ms=c["flush_physiques_ms"], ipc=c["total_avec_controles_locaux"],
                requetes_bloc=c["bloc"], dma=c["dma"], phases_moteur_ms=c["phases_moteur_ms"],
                ordonnanceur_invite=dict(repos_ms=c["repos_invite_ms"], actif_ms=c["actif_ordonnanceur_invite_ms"]),
                generations=[m["avant"], m["apres"]],
                pic_cadres_session=m["pic_cadres_session"], memoire_clients_kio=m["memoire_observee_kio"],
                sha256_destination=m["sha256_destination"], fil=debut["fil"], points_publication=q["echantillons"],
                points=samples)


def comparer(rows):
    groupes = defaultdict(list)
    for r in rows:
        groupes[(r["architecture"], r["cas"], r["secteur"], r["politique"])].append(r)
        groupes[(r["architecture"], r["cas"], None, r["politique"])].append(r)
    controles = {(r["architecture"], r["cas"], r["repetition"], r["secteur"]):r
                 for r in rows if r["politique"] == "ordinaire" and "erreur" not in r}
    resultats = []
    for (arch, cas, secteur, pol), bloc in groupes.items():
        valides = [r for r in bloc if "erreur" not in r]
        gains = []
        for r in valides:
            temoin = controles.get((arch, cas, r["repetition"], r["secteur"]))
            if temoin:
                gains.append(r["commande_ms"]-temoin["commande_ms"])
        coeurs = Counter()
        priorites = Counter()
        for r in valides:
            coeurs.update(r["points_publication"])
            priorites.update(p[5] for p in r["points"][2:])
        resultat = dict(architecture=arch, cas=cas, secteur=secteur, politique=pol, tentatives=len(bloc),
                        erreurs=len(bloc)-len(valides), echantillons_E_P=dict(coeurs), priorites_base=dict(priorites),
                        deltas_apparies_commande_ms=[round(x, 3) for x in gains],
                        delta_commande_ms=distribution(gains), depassements=sum(r["commande_ms"] >= 1000 for r in valides))
        for k in ("commande_ms", "cpu_vcpu_ms", "publication_ms", "apres_oui_ms"):
            resultat[k] = distribution(r[k] for r in valides)
        for i in range(2):
            resultat[f"flush_F{i+1}_ms"] = distribution(r["flush_physiques_ms"][i] for r in valides)
        resultats.append(resultat)
    return resultats


def table(groupes):
    titres = ["Architecture", "Édition", "Politique", "n", "Commande p50/p95/p99 (ms)", "Δ p50 apparié (ms)",
              "CPU TCG p50 (ms)", "Marqueurs E/P", "≥ 1 s"]
    lignes = [titres, ["---"]*len(titres)]
    francais = lambda x: f"{x:.1f}".replace(".", ",")
    for g in sorted((r for r in groupes if r["secteur"] is None),
                    key=lambda r: (ARCHS.index(r["architecture"]), CAS.index(r["cas"]), POLITIQUES.index(r["politique"]))):
        arch, cas, pol = g["architecture"], g["cas"], g["politique"]
        c = g["commande_ms"]
        if not c:
            lignes.append([arch, cas, pol, "0", "–", "–", "–", "–", "–"])
            continue
        coeur = g["echantillons_E_P"]
        lignes.append([arch, "mixte" if cas == CAS[0] else "locale", pol, str(c["n"]),
                       " / ".join(francais(c[p]) for p in ("p50", "p95", "p99")),
                       francais(g["delta_commande_ms"]["p50"]) if g["delta_commande_ms"] else "–",
                       francais(g["cpu_vcpu_ms"]["p50"]), f'{coeur.get("E",0)} / {coeur.get("P",0)}',
                       str(g["depassements"])])
    return "\n".join("| " + " | ".join(l) + " |" for l in lignes)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("campagne", type=Path)
    p.add_argument("sortie", type=Path)
    args = p.parse_args()
    env = json.loads((args.campagne/"environnement.json").read_bytes())
    rows = [reduire(json.loads(f.read_bytes())) for f in sorted(args.campagne.glob("*/essai.json"))]
    if not rows:
        raise ValueError("Aucun résultat")
    assert [r["ordre"] for r in rows] == list(range(1, len(rows)+1))
    groupes = comparer(rows)
    paires_threads = []
    for background in rows:
        if background["politique"] != "thread-background" or "erreur" in background:
            continue
        candidats = [r for r in rows if r["politique"] == "thread-user-initiated" and "erreur" not in r
                     and all(r[k] == background[k] for k in ("architecture", "cas", "secteur", "repetition"))]
        if not candidats:
            continue
        interactif, = candidats
        paires_threads.append(dict(architecture=background["architecture"], cas=background["cas"],
                                   secteur=background["secteur"], repetition=background["repetition"],
                                   ordre_background=background["ordre"], ordre_user_initiated=interactif["ordre"],
                                   delta_commande_ui_moins_bg_ms=interactif["commande_ms"]-background["commande_ms"],
                                   ratio_cpu_bg_sur_ui=background["cpu_vcpu_ms"]/interactif["cpu_vcpu_ms"],
                                   ratio_commande_bg_sur_ui=background["commande_ms"]/interactif["commande_ms"]))
    champs = ["type", "indice", "logical_cpu_id", "qos_demandee", "relative", "priorite_base", "priorite_courante",
              "cpu_ns_rel_S", "ecoule_ns_rel_S"]
    rapport = dict(base="262a335a01f4b66a0dba0d3a460cafb443f6eee6", environnement=env,
                   unites="Durées en millisecondes sauf suffixe _ns ; rang supérieur ceil(n*p).",
                   schema_points=champs, codes="I avant, Q après configuration à l’entrée du noyau ; S/T ouverture/fin de publication ; E entrée phase ; B0/B1 autour de F1, B2/B3 autour de F2.",
                   classes_darwin={"9":"background", "17":"utility", "21":"default", "25":"user-initiated"},
                   observations=rows, comparaisons=groupes, paires_threads=paires_threads)
    args.sortie.write_text(json.dumps(rapport, ensure_ascii=False, separators=(",",":"))+"\n")
    print(table(groupes))


if __name__ == "__main__":
    main()
