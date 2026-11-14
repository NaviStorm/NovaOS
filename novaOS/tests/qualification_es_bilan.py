"""Consolider les sauvegardes mesurées avec qualification_es_corrigees sur toute la matrice."""
import argparse
from collections import defaultdict
import json
from pathlib import Path
import re

from es_groupees_verifier_mesures import verifier_sonde
from qualification_es_corrigees import empreinte, examiner, programme
from qos_utm_analyse import phase
from vcpu_analyse import charge_sur_intervalle, percentiles


def observer(dossier, ident, temoin):
    fin = json.loads((dossier / "fin.json").read_bytes())
    fichier = dossier / "mesure.json"
    if fin["code"] or fin.get("refus") or fin["sha256_mesure"] != empreinte(fichier):
        raise ValueError(f"Mesure refusée ou empreinte incorrecte : {ident}")
    brut = json.loads(fichier.read_bytes())
    plateforme, secteur, cas = (fin[k] for k in ("plateforme", "secteur", "cas"))
    sonde = fin["mode"] in ("sonde", "trace-sonde")
    examiner(brut, plateforme, secteur, cas, sonde, fin["elf_sha256"])
    if fin["version"] == "avant" and fin["elf_sha256"] not in temoin[plateforme]:
        raise ValueError(f"Le binaire témoin diffère de l’audit initial : {ident}")
    m = brut["mesure"]
    ligne = dict(id=ident, **fin, commande_ms=m["sauvegarde_ms"],
                 pic_cadres_session=m["pic_cadres_session"], avant=m["avant"], apres=m["apres"],
                 sha256_destination=m["sha256_destination"],
                 loadavg_avant=brut["loadavg_avant"], loadavg_apres=brut["loadavg_apres"])
    if "memoire_observee_kio" in m:
        ligne["memoire_observee_kio"] = m["memoire_observee_kio"]
    if sonde:
        verifier_sonde(dict(essai=ident, **m))
        ligne["compteur"] = m["compteur"]
    if plateforme == "utm-aarch64":
        for cle in ("backend_hmp", "apres_oui_ms", "redemarrages", "octets_relus", "preallocation"):
            ligne[cle] = m[cle]
        image = m["image_importee"]
        ligne["qcow2"] = dict(**image["info"], provenance=image["provenance"],
                               octets_presents=image["octets_presents"], octets_absents=image["octets_absents"])
        qos = m["qos"]
        debut, _, fin_commande = m["commande_monotone_ns"]
        ligne["charge_mac"] = charge_sur_intervalle(qos["charge"], debut, fin_commande)
        ligne["politique_processus"] = qos["politique"]
        for borne in ("avant_commande", "apres_commande"):
            ligne["priorites_" + borne] = {
                nom: {k: qos[borne][nom][k] for k in ("nice", "priorite_ps", "etat_ps", "temps_cpu_ps")}
                for nom in ("utm", "qemu")
            }
        if fin["mode"].startswith("trace"):
            trace = qos["instruments"]
            releve = {k: phase(trace[k]) for k in ("avant_oui", "apres_oui", "commande")}
            releve.update({k: trace[k] for k in ("pid", "tid_hmp", "tid_instruments", "incertitude_par_borne_ns")})
            ligne["instruments"] = releve
            serie = dossier / "instruments/sauvegarde.serial"
            par_pid = {}
            for pid, privees, pile in re.findall(rb"CLI mem pid=(\d+) privees=(\d+) pile=(\d+)", serie.read_bytes()):
                cle = pid.decode("ascii")
                par_pid[cle] = max(par_pid.get(cle, 0), 4 * (int(privees) + int(pile)))
            ligne["memoire_observee_kio"] = par_pid
            ligne["sha256_serie_instruments"] = empreinte(serie)
    return ligne


def bilan(banc, repetitions):
    reference = Path(__file__).resolve().parents[1] / "docs/validation/es-v0.16.6-integration-mesures.json"
    temoin = defaultdict(set)
    for ligne in json.loads(reference.read_bytes())["essais"]:
        if not ligne["sonde"]:
            temoin[ligne["plateforme"]].add(ligne["elf_sha256"])
    lignes, groupes = [], defaultdict(list)
    for version, plateforme, secteur, cas, rep, mode in programme(repetitions):
        ident = f"{plateforme}-{secteur}-{cas}-{mode}-{version}-r{rep}"
        ligne = observer(banc / ident, ident, temoin)
        cle = (version, plateforme, secteur, cas, mode)
        if (*cle, rep) != tuple(ligne[k] for k in ("version", "plateforme", "secteur", "cas", "mode", "repetition")):
            raise ValueError(f"Cellule incorrecte : {ident}")
        lignes.append(ligne)
        groupes[cle].append(ligne["commande_ms"])
    stats = [dict(version=v, plateforme=p, secteur=s, cas=c, mode=m, **percentiles(valeurs))
             for (v, p, s, c, m), valeurs in sorted(groupes.items())]
    return lignes, stats


def ecrire(fichier, lignes, stats):
    def json_ligne(obj):
        return json.dumps(obj, ensure_ascii=False, allow_nan=False, separators=(",", ":"))

    methode = ("Horloge hôte depuis la saisie de :w vers un nom libre jusqu’au succès de vi après F2, "
               "accord inclus. Un seul invité à la fois. Percentiles au rang supérieur. "
               "Témoin, candidat hors sonde et sondes dans des VM distinctes.")
    with fichier.open("x", encoding="utf-8") as flux:
        flux.write('{\n  "methode": ' + json_ligne(methode))
        for cle, entrees in (("observations", lignes), ("statistiques", stats)):
            flux.write(',\n  "' + cle + '": [\n    ' + ',\n    '.join(map(json_ligne, entrees)) + '\n  ]')
        flux.write('\n}\n')


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("banc", type=Path)
    parser.add_argument("rapport", type=Path)
    parser.add_argument("--repetitions", type=int, default=3)
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error("au moins une répétition est nécessaire")
    ecrire(args.rapport, *bilan(args.banc, args.repetitions))
