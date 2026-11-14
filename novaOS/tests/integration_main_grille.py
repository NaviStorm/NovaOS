"""Comparer les deux chemins sur les mêmes vecteurs, une VM jetable à la fois."""
import argparse
from collections import defaultdict
import json
from pathlib import Path
import subprocess
import sys
import time

from reconstruire_es_0166 import CIBLES, empreinte

BENCH = Path(__file__).with_name("integration_main_performances.py")
PLATEFORMES = ("qemu-aarch64", "qemu-x86_64", "utm-aarch64")
CAS = ("neuf-4m", "inserer-milieu", "ajout-4m-multiple")


def programme():
    # Alterner l'ordre des deux chemins suivant la cellule.
    for plateforme in PLATEFORMES:
        for secteur in (512, 4096):
            for cas in CAS:
                ordre = ("classique", "groupe") if (secteur == 512) == (cas == "inserer-milieu") else ("groupe", "classique")
                for releve in ("ordinaire", "compteurs"):
                    for chemin in ordre:
                        yield plateforme, secteur, cas, releve, chemin
    for _ in range(2):
        for secteur in (512, 4096):
            for chemin in ("groupe", "classique"):
                yield "utm-aarch64", secteur, CAS[2], "ordinaire", chemin


def choisir_export(profils, temoins, utm, cas, releve, chemin):
    if chemin == "classique":
        if releve == "compteurs":
            return temoins / "compteurs"
        if cas == "inserer-milieu":
            return temoins / "distribution" if utm else profils / "brut"
        return temoins / "charge"
    if utm:
        return profils / ("utm-sonde" if releve == "compteurs" else "utm")
    return profils / ("sonde" if releve == "compteurs" else "sans")


def identite(profils, temoins, modele, plateforme, cas, releve, chemin):
    outils = profils / "sans/novaFS/target/aarch64-apple-darwin/release"
    utm = plateforme == "utm-aarch64"
    archive = choisir_export(profils, temoins, utm, cas, releve, chemin)
    provenance = archive / "provenance.json"
    objet = json.loads(provenance.read_bytes())
    target = "target-utm" if utm and chemin == "classique" else "target"
    triplet = CIBLES[0] if plateforme.endswith("aarch64") else CIBLES[1]
    elf = Path("novaOS") / target / triplet / "release/novaos"
    sha = empreinte(archive / elf)
    revision = objet.get("revision", objet.get("revision_git"))
    outils_objet = json.loads((profils / "sans/provenance.json").read_bytes())
    if sha != objet.get("noyaux", objet.get("binaires", {}))[str(elf)] or revision != outils_objet["revision_git"]:
        raise ValueError("La provenance du noyau ou la révision de son oracle divergent")
    attendu = dict(revision=revision, provenance_sha256=empreinte(provenance), noyau_sha256=sha)
    for nom, cle in (("novafs", "lecteur_sha256"), ("examples/es-groupees", "generateur_sha256")):
        fichier = outils / nom
        attendu[cle] = empreinte(fichier)
        if attendu[cle] != outils_objet["binaires"][str(fichier.relative_to(profils / "sans"))]:
            raise ValueError(f"Outil distinct de sa provenance {fichier}")
    fichiers = ("config.plist", "Data/code.fd", "Data/variables.fd")
    attendu["modele_utm"] = {k: empreinte(modele / k) for k in fichiers} if utm else None
    return archive, outils, attendu


def relire(dossier, ident, cellule, attendu):
    fin = json.loads((dossier / "fin.json").read_bytes())
    fiche = dossier / "mesure.json"
    if fin["code"] or fin["id"] != ident or fin["mesure_sha256"] != empreinte(fiche):
        raise ValueError(f"Mesure incomplète ou altérée {ident}")
    brut = json.loads(fiche.read_bytes())
    if any(brut.get(k) != v or k not in brut for k, v in attendu.items()):
        raise ValueError(f"Révision, profil ou binaires différents {ident}")
    if tuple(brut[k] for k in ("plateforme", "secteur", "cas", "releve", "chemin")) != cellule:
        raise ValueError(f"Mauvaise cellule {ident}")
    if brut["serial_sha256"] != empreinte(dossier / "sauvegarde.serial") or not 0 < brut["debut_utc_ns"] <= brut["fin_utc_ns"]:
        raise ValueError(f"Journal ou horodatage invalide {ident}")
    return brut, fin["mesure_sha256"]


def campagne(profils, temoins, modele, sortie, filtre):
    journaux = sortie / "journaux"
    journaux.mkdir(parents=True, exist_ok=True)
    erreurs = []
    selection = 0
    for i, cellule in enumerate(programme()):
        plateforme, secteur, cas, releve, chemin = cellule
        ident = f"{i:02}-{plateforme}-{secteur}-{cas}-{releve}-{chemin}"
        if filtre and filtre not in ident:
            continue
        selection += 1
        dossier = sortie / "serie" / ident
        fin = dossier / "fin.json"
        archive, outils, attendu = identite(profils, temoins, modele, plateforme, cas, releve, chemin)
        if fin.exists():
            relire(dossier, ident, cellule, attendu)
            continue
        utm = plateforme == "utm-aarch64"
        cmd = [sys.executable, "-B", BENCH, "mesurer", plateforme, chemin, releve, str(secteur),
               cas, archive, outils / "examples/es-groupees", outils / "novafs", dossier]
        if utm:
            cmd.extend(["--profil-utm", modele])
        print(time.strftime("%H:%M:%S"), ident, flush=True)
        with (journaux / (ident + ".log")).open("x") as flux:
            code = subprocess.run(cmd, stdout=flux, stderr=subprocess.STDOUT).returncode
        dossier.mkdir(exist_ok=True, parents=True)
        fichier = dossier / "mesure.json"
        sha = empreinte(fichier) if fichier.is_file() else None
        fin.write_text(json.dumps(dict(code=code, cmd=[str(t) for t in cmd], id=ident, mesure_sha256=sha)) + "\n")
        if code:
            erreurs.append(ident)
            print("Échec", ident, flush=True)
        else:
            relire(dossier, ident, cellule, attendu)
    if selection == 0:
        raise SystemExit(f"Filtre sans cellule : {filtre}")
    if erreurs:
        raise SystemExit("Échecs conservés dans les journaux : " + ", ".join(erreurs))


def bilan(profils, temoins, modele, sortie, rapport):
    from vcpu_analyse import charge_sur_intervalle, percentiles
    observations, groupes = [], defaultdict(list)
    for i, cellule in enumerate(programme()):
        plateforme, secteur, cas, releve, chemin = cellule
        ident = f"{i:02}-{plateforme}-{secteur}-{cas}-{releve}-{chemin}"
        dossier = sortie / "serie" / ident
        _, _, attendu = identite(profils, temoins, modele, plateforme, cas, releve, chemin)
        brut, sha = relire(dossier, ident, cellule, attendu)
        m = brut["mesure"]
        obs = {k: v for k, v in brut.items() if k != "mesure"}
        obs.update(id=ident, mesure_sha256=sha)
        for cle in ("avant", "apres", "pic_cadres_session", "sha256_destination", "sauvegarde_ms", "apres_oui_ms"):
            obs[cle] = m[cle]
        cpu = m.get("cpu_hote_qemu") or m.get("qos", {}).get("cpu_hote_qemu")
        if cpu:
            obs["cpu_hote_qemu"] = cpu
        if releve == "compteurs":
            obs["compteur"] = m["compteur"]
        if plateforme == "utm-aarch64":
            for cle in ("backend_hmp", "redemarrages", "octets_relus", "image_initiale", "politique_demandee"):
                obs[cle] = m[cle]
            debut, _, acquit = m["commande_monotone_ns"]
            obs["charge_hote"] = charge_sur_intervalle(m["qos"]["charge"], debut, acquit)
            obs["serial_reprise_sha256"] = empreinte(dossier / "reprise.serial")
            obs["moniteur_sha256"] = empreinte(dossier / "moniteur.log")
        observations.append(obs)
        groupes[(plateforme, secteur, cas, chemin, releve)].append(m["sauvegarde_ms"])
    stats = [dict(plateforme=p, secteur=s, cas=c, chemin=h, releve=r, **percentiles(temps))
             for (p, s, c, h, r), temps in sorted(groupes.items())]
    with rapport.open("x", encoding="utf-8") as flux:
        flux.write("{\n")
        for i, (nom, lignes) in enumerate((("observations", observations), ("statistiques_ms", stats))):
            if i:
                flux.write(",\n")
            chaines = [json.dumps(l, ensure_ascii=False, allow_nan=False, separators=(",", ":")) for l in lignes]
            flux.write(f'  "{nom}": [\n    ' + ",\n    ".join(chaines) + "\n  ]")
        flux.write("\n}\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    sous = p.add_subparsers(dest="action", required=True)
    c = sous.add_parser("campagne")
    c.add_argument("--filtre", help="Sous-chaîne du nom des cellules à lancer")
    b = sous.add_parser("bilan")
    for s in (c, b):
        for champ in ("profils", "temoins", "modele_utm", "sortie"):
            s.add_argument(champ, type=Path)
    b.add_argument("rapport", type=Path)
    a = p.parse_args()
    if a.action == "campagne":
        campagne(a.profils.resolve(), a.temoins.resolve(), a.modele_utm.resolve(), a.sortie.resolve(), a.filtre)
    else:
        bilan(a.profils.resolve(), a.temoins.resolve(), a.modele_utm.resolve(), a.sortie.resolve(), a.rapport)
