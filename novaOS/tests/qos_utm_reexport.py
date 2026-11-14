"""Récupérer hors ligne les intervalles d’une trace UTM après une erreur Instruments."""
import argparse
from datetime import datetime, timezone
import fcntl
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import time
from xml.etree.ElementTree import ParseError

from qos_observation import analyse, exporter_xml
from qos_utm import enregistrer, nom, verifier


def source(fichier):
    contenu = fichier.read_bytes()
    fiche = json.loads(contenu)
    mesures, essai = fiche["mesures"], fiche["essai"]
    verifier(mesures, essai)
    qos = mesures["qos"]
    if (fichier.stem != nom(essai) or not essai["trace"] or "erreur" in fiche
            or not qos.get("erreur_instruments") or "instruments" in qos):
        raise ValueError("Seul un essai relu avec une erreur Instruments est admissible")
    if mesures["commande_monotone_ns"] != [h[2] for h in qos["horloges"]]:
        raise ValueError("Les ancrages de la fiche divergent des bornes de commande")
    return mesures, dict(essai=fichier.stem, fiche_sha256=hashlib.sha256(contenu).hexdigest(),
                         erreur_instruments_initiale=qos["erreur_instruments"])


def reconstruire(export, m):
    q = m["qos"]
    r = analyse(export, q["pid_moteur"], q["tid_moniteur"], m["commande_monotone_ns"],
                q["horloges"], pid_utm=q["pid_utm"])
    phase = r["commande"]
    if phase["duree_sans_etat_ns"] or phase["cpu_par_coeur_ns"].get("inconnu"):
        raise ValueError("Couverture de la commande ou de la résidence P/E incomplète")
    return r


def empreintes(export):
    return {nom: hashlib.sha256((export/nom).read_bytes()).hexdigest()
            for nom in ("trace-toc.xml", "trace-etats.xml", "trace-qos.xml")}


def lire(fichier):
    """Retourne le résultat latéral après relecture des preuves, ou None s’il est absent."""
    dossier = fichier.with_suffix("") / "post-traitement"
    resultat = dossier / "resultat.json"
    if not resultat.exists():
        return None
    mesures, identite = source(fichier)
    r = json.loads(resultat.read_bytes())
    if any(r[k] != v for k, v in identite.items()):
        raise ValueError(f"Le résultat de récupération ne correspond pas à {fichier}")
    tentative = r["tentative"]
    if not tentative.startswith("export-") or Path(tentative).name != tentative:
        raise ValueError("Chemin d’export Instruments invalide")
    export = dossier / tentative
    if r != json.loads((export/"resultat.json").read_bytes()):
        raise ValueError("Le bilan ne correspond pas à sa tentative d’export")
    if r["statut"] == "recupere":
        if r["xml_sha256"] != empreintes(export) or r["instruments"] != reconstruire(export, mesures):
            raise ValueError("La relecture des preuves Instruments diverge")
    elif r["statut"] != "echec" or not r["diagnostic"]:
        raise ValueError("Statut de récupération inconnu")
    return r


def reexporter(fichier):
    mesures, r = source(fichier)
    dossier = fichier.with_suffix("")
    sorties = dossier/"post-traitement"
    sorties.mkdir(exist_ok=True)
    with (sorties/"export.lock").open("a") as verrou:
        fcntl.flock(verrou, fcntl.LOCK_EX)
        precedent = lire(fichier)
        if precedent and precedent["statut"] == "recupere":
            return precedent
        return tentative(dossier, sorties, mesures, r)


def tentative(dossier, sorties, mesures, r):
    export = Path(tempfile.mkdtemp(prefix="export-", dir=sorties))
    r.update(tentative=export.name, debut_utc=datetime.now(timezone.utc).isoformat())
    debut = time.monotonic_ns()
    try:
        exporter_xml(dossier/"systeme.trace", export)
        r.update(statut="recupere", instruments=reconstruire(export, mesures), xml_sha256=empreintes(export))
    except (OSError, ValueError, KeyError, ParseError, subprocess.SubprocessError) as exc:
        stderr = getattr(exc, "stderr", None)
        detail = stderr.decode(errors="replace").strip() if stderr else ""
        r.update(statut="echec", diagnostic=f"{type(exc).__name__} {exc} {detail}".strip())
    r["export_analyse_ns"] = time.monotonic_ns()-debut
    enregistrer(export/"resultat.json", r)
    enregistrer(sorties/"resultat.json", r)
    return r


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("fiche", type=Path)
    f = p.parse_args().fiche
    r = reexporter(f)
    print(r["statut"], f.with_suffix("")/"post-traitement"/"resultat.json")
    if r["statut"] == "echec":
        p.exit(1, r["diagnostic"]+"\n")
