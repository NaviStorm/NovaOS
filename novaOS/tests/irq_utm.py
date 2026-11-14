"""Comparer par paires deux ELF UTM sur la même sauvegarde et la même charge Darwin."""
import argparse
from collections import defaultdict
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

from vcpu_analyse import analyser, percentiles


# (scénario, secteur, charge, politique demandée, sonde invitée, paires)
MIXTE = "ajout-4m-multiple"
CELLULES = (
    (MIXTE, 4096, "stockage", "ordinaire", False, 20),
    (MIXTE, 4096, "stockage", "-b", False, 8),
    (MIXTE, 4096, "repos", "ordinaire", False, 6),
    ("inserer-debut", 512, "repos", "ordinaire", False, 4),
    (MIXTE, 4096, "stockage", "-b", True, 4),
    (MIXTE, 512, "repos", "ordinaire", True, 4),
    ("inserer-debut", 512, "repos", "ordinaire", True, 2),
)


def programme():
    for rep in range(max(c[-1] for c in CELLULES)):
        for pas in range(len(CELLULES)):
            idx = (pas + rep) % len(CELLULES)
            cas, secteur, charge, politique, sonde, nombre = CELLULES[idx]
            if rep >= nombre:
                continue
            paire = f"c{idx+1}-r{rep+1:02}"
            ordre = ("polling", "irq") if (rep+idx)%2 == 0 else ("irq", "polling")
            for rang, mode in enumerate(ordre, 1):
                yield dict(paire=paire, rang=rang, mode=mode, cas=cas, secteur=secteur,
                           charge=charge, politique=politique, sonde=sonde)


def lire_latences(journal, compteur):
    texte = journal.read_text()
    (nombre,) = re.findall(r"^SAVE latences_bloc n=(\d+)$", texte, re.M)
    codes = []
    for debut, vecteur in re.findall(r"^SAVE latences_bloc debut=(\d+) valeurs=(\[[^\]]+\])$", texte, re.M):
        if int(debut) != len(codes):
            raise ValueError("Offsets de la trace bloc discontinus")
        codes.extend(json.loads(vecteur))
    if len(codes) != int(nombre) or len(codes) > 256:
        raise ValueError("Trace des requêtes bloc incomplète")
    (frequence,) = re.findall(r"^BLOC memoire [^\n]* hz=(\d+)", texte, re.M)
    hz = int(frequence)
    if hz == 0:
        raise ValueError("Fréquence du compteur bloc nulle")
    noms = ["lecture", "ecriture", "flush"]
    ops = {k: [] for k in noms}
    for code in codes:
        if type(code) is not int or not 0 <= code < 1 << 64:
            raise ValueError("Commande bloc mal encodée")
        duree, op = divmod(code, 4)
        if op >= len(noms) or duree <= 0:
            raise ValueError("Durée ou opération VirtIO invalide")
        ops[noms[op]].append(duree)
    (vecteur,) = re.findall(r"^SAVE phases=\[[^\]]+\] bloc=(\[[^\]]+\])$", texte, re.M)
    compteurs_ticks = [n for valeurs in ops.values() for n in (len(valeurs), sum(valeurs))]
    if json.loads(vecteur) != compteurs_ticks:
        raise ValueError("La trace individuelle diverge des compteurs bloc en ticks")
    (barrieres,) = re.findall(r"^SAVE repos=\d+ flush_physiques=(\[[^\]]+\])$", texte, re.M)
    if ops["flush"] != json.loads(barrieres):
        raise ValueError("Les durées des FLUSH F1/F2 divergent de la trace ordonnée")
    for nom, valeurs in ops.items():
        b = compteur["bloc"][nom]
        if len(valeurs) != b["nombre"] or round(sum(valeurs)*1000/hz, 3) != b["millisecondes"]:
            raise ValueError("La trace individuelle diverge des compteurs bloc")
    return dict(hz=hz, noms=noms, commandes_ticks_type=codes)


def sauver(fichier, valeur):
    temporaire = fichier.with_suffix(".tmp")
    temporaire.write_text(json.dumps(valeur, ensure_ascii=False, separators=(",", ":"), allow_nan=False)+"\n")
    temporaire.replace(fichier)


def verifier(m, essai):
    if (m["cas"], m["secteur"], m["charge_hote"]["profil"], m["politique_demandee"], m["sonde"]) != (
            essai["cas"], essai["secteur"], essai["charge"], essai["politique"], essai["sonde"]):
        raise ValueError("Conditions différentes de la cellule prévue")
    if (m["environnement"] != "UTM QCOW2" or m["preallocation"] is not False
            or m["image_importee"].get("info", {}).get("format") != "qcow2"
            or m["image_importee"].get("octets_absents", 0) <= 0):
        raise ValueError("Profil QCOW2 clairsemé non confirmé sur le disque importé")
    if not (m["redemarrages"] == 1 and m["octets_relus"] and m["apres"][-1] > m["avant"][-1] and
            re.fullmatch(r"[a-f0-9]{64}", m["sha256_destination"]) and
            m["backend_hmp"]["flush_operations"] == 2 and m["image_importee"]["disponible"]):
        raise ValueError("Preuve de relecture, d’allocation ou de FLUSH manquante")
    if essai["sonde"]:
        mesures = m["compteur"]
        if ("attente_irq" in mesures) != (essai["mode"] == "irq"):
            raise ValueError("Compteurs de réveil incohérents avec le mode sélectionné")
        if mesures["total_avec_controles_locaux"] > 64:
            raise ValueError("Plafond de 64 échanges dépassé")


def verifier_fiche(fiche, dossier, essai):
    releves = fiche["releves"]
    verifier(releves, essai)
    if releves != json.loads((dossier/"resultats.json").read_bytes()):
        raise ValueError(f"Les relevés archivés divergent du résultat brut dans {dossier}")
    if fiche["analyse"] != analyser(releves):
        raise ValueError(f"L’analyse archivée diverge des horodatages dans {dossier}")
    if essai["sonde"]:
        if fiche["requetes"] != lire_latences(dossier/"sauvegarde.serial", releves["compteur"]):
            raise ValueError(f"La trace bloc archivée diverge du journal dans {dossier}")
    elif "requetes" in fiche:
        raise ValueError(f"Trace de requêtes inattendue dans un essai sans sonde dans {dossier}")


def campagne(a):
    a.sortie.mkdir(exist_ok=True, parents=True)
    banc = Path(__file__).with_name("es_groupees_utm.py")
    elfs = {("polling", False): a.polling, ("irq", False): a.irq,
            ("polling", True): a.polling_sonde, ("irq", True): a.irq_sonde}
    sha = {k: hashlib.sha256(v.read_bytes()).hexdigest() for k, v in elfs.items()}
    if len(set(sha.values())) != 4:
        raise ValueError("Quatre ELF distincts attendus")
    for essai in programme():
        mode = essai["mode"]
        cle = (mode, essai["sonde"])
        nom = f"{essai['paire']}-{mode}"
        dossier = a.sortie / nom
        preuve = dossier / "qualification.json"
        incident = a.sortie / (nom + ".echec.json")
        if incident.exists():
            echec = json.loads(incident.read_bytes())
            if preuve.exists() or echec["essai"] != essai or echec["elf_sha256"] != sha[cle]:
                raise ValueError(f"Témoins divergents pour {nom}")
            continue
        if preuve.exists():
            m = json.loads(preuve.read_bytes())
            if m["essai"] != essai or m["elf_sha256"] != sha[cle]:
                raise ValueError(f"Configuration divergente pour {nom}")
            verifier_fiche(m, dossier, essai)
            continue
        cmd = [sys.executable, "-B", banc, a.profil, elfs[cle], a.generateur, dossier,
               str(essai["secteur"]), "--cas", essai["cas"], "--profil-charge", essai["charge"]]
        if essai["politique"] == "-b":
            cmd.append("--arriere-plan")
        if not essai["sonde"]:
            cmd.append("--sans-sonde")
        debut = datetime.now(timezone.utc).isoformat()
        log = dossier.with_suffix(".log")
        fiche = dict(essai=essai, elf_sha256=sha[cle], debut_utc=debut)
        with log.open("x") as journal:
            proc = subprocess.run(cmd, stdout=journal, stderr=subprocess.STDOUT)
        try:
            if proc.returncode:
                raise ValueError(f"Le banc UTM a quitté avec le code {proc.returncode}")
            mesures = json.loads((dossier/"resultats.json").read_bytes())
            fiche["releves"] = mesures
            verifier(mesures, essai)
            fiche["analyse"] = analyser(mesures)
            if essai["sonde"]:
                fiche["requetes"] = lire_latences(dossier/"sauvegarde.serial", mesures["compteur"])
        except (OSError, ValueError, KeyError, TypeError, AssertionError) as erreur:
            fiche.update(erreur=str(erreur), fin_utc=datetime.now(timezone.utc).isoformat(),
                         diagnostic=log.read_bytes()[-4096:].decode(errors="replace"))
            sauver(incident, fiche)
            print(nom, fiche["erreur"], flush=True)
            continue
        sauver(preuve, fiche)
        print(nom, mesures["sauvegarde_ms"], flush=True)


def cpu_vcpu(releve):
    niveaux = releve["vcpu"]["niveaux_cpu"].get("deltas")
    if niveaux:
        return sum(v["cpu_ns"] for v in niveaux)
    return None


def coeur_dominant(releve):
    niveaux = releve["vcpu"]["niveaux_cpu"].get("deltas") or []
    total = sum(v["cpu_ns"] for v in niveaux)
    for niveau in niveaux:
        if total and niveau["cpu_ns"] >= 0.9 * total:
            return niveau["nom"]
    return "mixte" if total else "inconnu"


def statistiques_bloc(essais):
    latences = defaultdict(list)
    for fiche in essais:
        trace = fiche["requetes"]
        for valeur in trace["commandes_ticks_type"]:
            ticks, op = divmod(valeur, 4)
            latences[trace["noms"][op]].append(round(ticks * 10**9 / trace["hz"]))
    return {k: percentiles(valeurs) for k, valeurs in latences.items()}


def rapport(a):
    cellules = defaultdict(dict)
    sorties = []
    incidents = []
    for essai in programme():
        nom = essai["paire"] + "-" + essai["mode"]
        incident = a.dossier / (nom + ".echec.json")
        if incident.is_file():
            fiche = json.loads(incident.read_bytes())
            if fiche["essai"] != essai or (a.dossier/nom/"qualification.json").exists():
                raise ValueError(f"Témoins divergents pour {nom}")
            incidents.append(fiche)
            continue
        fiche = json.loads((a.dossier/nom/"qualification.json").read_bytes())
        if fiche["essai"] != essai:
            raise ValueError(f"Identité de la paire divergente dans {nom}")
        verifier_fiche(fiche, a.dossier/nom, essai)
        sorties.append(fiche)
        groupe = cellules[essai["paire"].split("-")[0]]
        groupe.setdefault(essai["paire"], {})[essai["mode"]] = fiche
    resumes = []
    for cellule, paires in cellules.items():
        resume = dict(cellule=cellule, paires_completes=[k for k,p in paires.items() if len(p)==2])
        for mode in ("polling", "irq"):
            essais = [paire[mode] for paire in paires.values() if mode in paire]
            if not essais:
                continue
            mur = [f["analyse"]["commande_ns"] for f in essais]
            cpu = [t for f in essais if (t := cpu_vcpu(f["releves"])) is not None]
            brut = [f["releves"] for f in essais]
            tableau = dict(commande_ns=percentiles(mur), cpu_vcpu_ns=percentiles(cpu),
                           cpu_indisponible=len(essais)-len(cpu),
                           au_moins_une_seconde=sum(t >= 10**9 for t in mur),
                           pic_cadres_session=percentiles([m["pic_cadres_session"] for m in brut]),
                           repartition_90_pct={
                               k: sum(coeur_dominant(m) == k for m in brut)
                               for k in ("Performance", "Efficiency", "mixte", "inconnu")})
            if essais[0]["essai"]["sonde"]:
                tableau["requetes_bloc_ns"] = statistiques_bloc(essais)
                for cle in ("actif_ordonnanceur_invite_ms", "repos_invite_ms", "total_avec_controles_locaux"):
                    tableau[cle] = percentiles([m["compteur"][cle] for m in brut])
                if mode == "irq":
                    tableau["attente_irq_totaux"] = {
                        k: sum(m["compteur"]["attente_irq"][k] for m in brut)
                        for k in brut[0]["compteur"]["attente_irq"]}
            resume[mode] = tableau
        ecarts, ecarts_cpu = [], []
        meme_niveau = defaultdict(list)
        for p in paires.values():
            if len(p) != 2:
                continue
            deux = [p[k]["releves"] for k in ("polling", "irq")]
            for cle in ("avant", "apres", "sha256_destination", "octets_relus"):
                if deux[0][cle] != deux[1][cle]:
                    raise ValueError(f"Relectures différentes dans la paire {p['irq']['essai']['paire']}")
            ecarts.append(p["irq"]["analyse"]["commande_ns"] - p["polling"]["analyse"]["commande_ns"])
            cpu = [cpu_vcpu(x) for x in deux]
            if None not in cpu:
                ecarts_cpu.append(cpu[1] - cpu[0])
                niveau = coeur_dominant(deux[0])
                if niveau == coeur_dominant(deux[1]) and niveau in ("Efficiency", "Performance"):
                    meme_niveau[niveau].append(cpu[1] - cpu[0])
        resume.update(delta_commande_irq_moins_polling_ns=percentiles(ecarts),
                      delta_cpu_irq_moins_polling_ns=percentiles(ecarts_cpu),
                      paires_cpu_en_baisse=sum(d < 0 for d in ecarts_cpu),
                      paires_cpu_disponible=len(ecarts_cpu),
                      delta_cpu_par_niveau_90_pct_ns={k: percentiles(v) for k,v in meme_niveau.items()})
        resumes.append(resume)
    sauver(a.sortie, dict(unite_temps="ns, sauf suffixe _ms", rang="ceil(n × p)",
                          comparaison="IRQ moins polling, paires de même cellule et répétition",
                          cellules=resumes, essais=sorties, echecs=incidents))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    sous = p.add_subparsers(required=True)
    lancer = sous.add_parser("campagne")
    for arg in ("profil", "generateur", "polling", "irq", "polling_sonde", "irq_sonde", "sortie"):
        lancer.add_argument(arg, type=Path)
    lancer.set_defaults(fonction=campagne)
    lire = sous.add_parser("rapport")
    lire.add_argument("dossier", type=Path)
    lire.add_argument("sortie", type=Path)
    lire.set_defaults(fonction=rapport)
    args = p.parse_args()
    args.fonction(args)
