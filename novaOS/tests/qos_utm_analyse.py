"""Dépouillement reproductible des essais UTM à politique hôte contrôlée."""
import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import re

from qos_utm import nom, programme, verifier
from qos_utm_reexport import lire as lire_reexport
from vcpu_analyse import charge_sur_intervalle, percentiles


def centiemes_ps(valeur):
    # ps Darwin emploie usuellement M:SS.cc, avec heures/jours pour les
    # processus anciens. Conserver les centièmes entiers jusqu'à la différence.
    m = re.fullmatch(r"(?:(\d+)-)?(?:(\d+):)?(\d+):(\d{2})\.(\d{2})", valeur)
    if not m:
        return None
    j, h, minute, sec, fraction = (int(v or 0) for v in m.groups())
    return (((j*24+h)*60+minute)*60+sec)*100+fraction


def activite_processus(qos):
    if not all(k in qos for k in ("avant_commande", "apres_commande")):
        return {}
    avant, apres = qos["avant_commande"], qos["apres_commande"]
    bilan = {}
    for k in ("utm", "qemu"):
        a, b = avant[k], apres[k]
        cpu_a = centiemes_ps(a.get("temps_cpu_ps", ""))
        cpu_b = centiemes_ps(b.get("temps_cpu_ps", ""))
        if a["pid"] == b["pid"] and cpu_a is not None and cpu_b is not None and cpu_b >= cpu_a:
            bilan[k] = (cpu_b - cpu_a)*10
    return bilan


def phase(trace):
    etats = trace["etats_ns"]
    coeurs = trace["cpu_par_coeur_ns"]
    duree = trace["fin_trace_ns"] - trace["debut_trace_ns"]
    cpu = etats.get("Running", 0)
    if (duree <= 0 or trace["duree_sans_etat_ns"] < 0 or sum(coeurs.values()) != cpu
            or any(n < 0 for n in (*etats.values(), *coeurs.values()))
            or sum(etats.values()) + trace["duree_sans_etat_ns"] != duree):
        raise ValueError("Comptabilité temporelle Instruments incohérente")
    return dict(cpu_ms=cpu/1e6, e_ms=coeurs.get("E", 0)/1e6, p_ms=coeurs.get("P", 0)/1e6,
                e_pct=coeurs.get("E", 0)*100/cpu if cpu else None,
                runnable_ms=etats.get("Runnable", 0)/1e6,
                blocked_ms=etats.get("Blocked", 0)/1e6,
                preempted_ms=etats.get("Preempted", 0)/1e6,
                interrupted_ms=etats.get("Interrupted", 0)/1e6,
                inconnu_ms=trace["duree_sans_etat_ns"]/1e6)


def analyser(dossier, repos_trace=False):
    attendus = {nom(c): c for c in programme(repos_trace)}
    essais, echecs = [], []
    groupes = defaultdict(list)
    paires = defaultdict(dict)
    par_placement = defaultdict(list)
    for fichier in sorted(dossier.glob("c*-r*-*.json")):
        brut = json.loads(fichier.read_bytes())
        ident = fichier.stem
        if ident not in attendus or brut["essai"] != attendus[ident]:
            raise ValueError(f"Essai étranger au programme dans {fichier}")
        if "erreur" in brut:
            echecs.append(dict(id=ident, **brut))
            continue
        c, m = brut["essai"], brut["mesures"]
        verifier(m, c)
        analyse = dict(delta_cpu_ps_processus_ms=activite_processus(m["qos"]),
                       charge=charge_sur_intervalle(m["qos"]["charge"],
                                                   m["commande_monotone_ns"][0],
                                                   m["commande_monotone_ns"][2]))
        trace = m["qos"].get("instruments")
        if c["trace"] and trace is None and "erreur_instruments" in m["qos"]:
            recuperation = lire_reexport(fichier)
            if recuperation is not None:
                analyse["reexport"] = recuperation
                if recuperation["statut"] == "recupere":
                    trace = recuperation["instruments"]
        if trace is not None:
            analyse["phases"] = {p: phase(trace[p])
                                for p in ("commande", "avant_oui", "apres_oui")}
            cpu = trace["commande"]["cpu_par_coeur_ns"]
            running = trace["commande"]["etats_ns"].get("Running", 0)
            if running and not cpu.get("inconnu") and not trace["commande"]["duree_sans_etat_ns"]:
                placement = "mixte"
                for coeur in ("E", "P"):
                    if 10*cpu.get(coeur, 0) >= 9*running:
                        placement = coeur
                analyse["placement"] = placement
                par_placement[(c["cellule"], c["politique"], placement)].append(ident)
        elif c["trace"] and "erreur_instruments" not in m["qos"]:
            raise ValueError(f"Trace demandée absente sans diagnostic pour {ident}")
        preuves = {}
        for motif in ("*.serial", "moniteur.log", "trace*.xml", "trace.log",
                      "post-traitement/export-*/*.xml", "post-traitement/resultat.json"):
            for preuve in sorted((dossier/ident).glob(motif)):
                preuves[str(preuve.relative_to(dossier/ident))] = hashlib.sha256(preuve.read_bytes()).hexdigest()
        essai = dict(id=ident, analyse=analyse, preuves_sha256=preuves, **brut)
        essais.append(essai)
        groupes[(c["cellule"], c["politique"])].append(essai)
        paires[(c["cellule"], c["repetition"])][c["politique"]] = essai
    cellules = []
    for (indice, politique), groupe in sorted(groupes.items()):
        mesures = [e["mesures"] for e in groupe]
        cellule = dict(**{k: groupe[0]["essai"][k] for k in
                          ("cellule", "politique", "cas", "secteur", "charge", "trace", "sonde")},
                       essais=[e["id"] for e in groupe],
                       commande_ms=percentiles([m["sauvegarde_ms"] for m in mesures]),
                       apres_oui_ms=percentiles([m["apres_oui_ms"] for m in mesures]),
                       au_moins_1s=sum(m["commande_monotone_ns"][2]-m["commande_monotone_ns"][0] >= 10**9
                                       for m in mesures),
                       pic_cadres_session=percentiles([m["pic_cadres_session"] for m in mesures]),
                       cpu_global_actif_pct=percentiles([
                           e["analyse"]["charge"]["cpu_global_actif_pct"] for e in groupe
                           if e["analyse"]["charge"]["cpu_global_actif_pct"] is not None]),
                       backend_hmp={k: percentiles([m["backend_hmp"][k] for m in mesures])
                                    for k in mesures[0]["backend_hmp"]})
        observes = [e for e in groupe if "phases" in e["analyse"]]
        cellule["traces_exploitees"] = len(observes)
        cellule["phases"] = {
            p: {k: percentiles([e["analyse"]["phases"][p][k] for e in observes
                               if e["analyse"]["phases"][p][k] is not None])
                for k in observes[0]["analyse"]["phases"][p]}
            for p in ("commande", "avant_oui", "apres_oui")} if observes else {}
        if cellule["sonde"]:
            cellule["ipc"] = percentiles([m["compteur"]["total_avec_controles_locaux"] for m in mesures])
            cellule["F1_ms"] = percentiles([m["compteur"]["flush_physiques_ms"][0] for m in mesures])
            cellule["F2_ms"] = percentiles([m["compteur"]["flush_physiques_ms"][1] for m in mesures])
        cellules.append(cellule)
    comparaisons = []
    for (cellule, repetition), serie in sorted(paires.items()):
        if set(serie) != {"ordinaire", "-b", "-B"}:
            continue
        temps = {p: e["mesures"]["sauvegarde_ms"] for p, e in serie.items()}
        comparaison = dict(cellule=cellule, repetition=repetition, commande_ms=temps,
                           fond_moins_ordinaire_ms=temps["-b"]-temps["ordinaire"],
                           sortie_moins_ordinaire_ms=temps["-B"]-temps["ordinaire"])
        if all("phases" in e["analyse"] for e in serie.values()):
            cpu = {p: e["analyse"]["phases"]["commande"]["cpu_ms"] for p, e in serie.items()}
            comparaison["vcpu_running_ms"] = cpu
            comparaison["ecoule_moins_running_ms"] = {p: temps[p]-cpu[p] for p in serie}
            comparaison["vcpu_e_pct_du_running"] = {
                p: e["analyse"]["phases"]["commande"]["e_pct"] for p, e in serie.items()}
        comparaisons.append(comparaison)
    presents = {e["id"] for e in essais+echecs}
    return dict(methode="Percentiles au rang supérieur ceil(p*n), par cellule, sans exclusion de valeurs. "
                        "Les politiques processus -b/-B ont aussi un effet sur les E/S. "
                        "Les temps hôtes couvrent la commande :w et sa confirmation automatique. "
                        "Les états Instruments sont découpés aux mêmes bornes, avec l’incertitude publiée.",
                planifies=len(attendus), manquants=sorted(set(attendus)-presents),
                echecs=echecs, cellules=cellules, comparaisons_appariees=comparaisons,
                par_placement_90_pct=[dict(cellule=c, politique=p, placement=h, essais=ids)
                                      for (c, p, h), ids in sorted(par_placement.items())],
                essais=essais)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("dossier", type=Path)
    p.add_argument("sortie", type=Path)
    p.add_argument("--repos-trace", action="store_true", help="inclure la cellule complémentaire au repos")
    a = p.parse_args()
    if a.sortie.resolve().parent == a.dossier.resolve():
        p.error("Conserver le rapport en dehors du dossier des relevés bruts")
    a.sortie.write_text(json.dumps(analyser(a.dossier, a.repos_trace), ensure_ascii=False,
                                  allow_nan=False, separators=(",", ":"))+"\n")
