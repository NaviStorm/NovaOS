"""Mesurer chaque cellule sur la pile de référence et le candidat, à charge séquentielle."""
import argparse
from datetime import datetime, timezone
import hashlib
import itertools
import json
from pathlib import Path
import subprocess
import sys

PLATEFORMES = ("qemu-aarch64", "qemu-x86_64", "utm-aarch64")
CAS = ("neuf-4m", "inserer-milieu", "ajout-4m-multiple")
BANC = Path(__file__).with_name("integration_mesure.py")


def programme(repetitions):
    cellules = list(itertools.product(PLATEFORMES, (512, 4096), CAS))
    for rep in range(1, repetitions + 1):
        for i, (plateforme, secteur, cas) in enumerate(cellules):
            # Premier passage apparié. Alterner la version exécutée en premier.
            versions = ("avant", "apres") if i % 2 == 0 else ("apres", "avant")
            for version in versions if rep == 1 else ("apres",):
                yield version, plateforme, secteur, cas, rep, "normal"
            if rep == 1:
                yield "apres", plateforme, secteur, cas, rep, "sonde"
    for secteur in (512, 4096):
        for mode in ("trace", "trace-sonde"):
            yield "apres", "utm-aarch64", secteur, "ajout-4m-multiple", 1, mode


def empreinte(fichier):
    with fichier.open("rb") as flux:
        return hashlib.file_digest(flux, "sha256").hexdigest()


def examiner(mesure, plateforme, secteur, cas, sonde, sha):
    if mesure["plateforme"] != plateforme or mesure["elf_sha256"] != sha:
        raise ValueError("Identité du noyau ou de la plateforme incorrecte")
    m = mesure["mesure"]
    if (m["cas"], m["secteur"], m["sonde"]) != (cas, secteur, sonde):
        raise ValueError("Cellule de mesure incorrecte")
    if not m["sha256_destination"] or m["apres"][-1] <= m["avant"][-1]:
        raise ValueError("Relecture ou avancée de génération absente")
    if sonde:
        c = m["compteur"]
        if not c.get("lots") or c["bloc"]["flush"]["nombre"] != 2:
            raise ValueError("Chemin groupé avec deux barrières absent")
        if c["total_avec_controles_locaux"] > 64:
            raise ValueError("Plafond IPC dépassé")
    if plateforme == "utm-aarch64":
        if m["redemarrages"] != 1 or not m["octets_relus"] or m["backend_hmp"]["flush_operations"] != 2:
            raise ValueError("Reprise UTM ou FLUSH non confirmé")


def lancer(a):
    a.sortie.mkdir(parents=True, exist_ok=True)
    sha = {}
    for version, plateforme, secteur, cas, rep, mode in programme(a.repetitions):
        moteur, arch = plateforme.split("-")
        sonde = mode in ("sonde", "trace-sonde")
        trace = mode.startswith("trace")
        if version == "avant":
            archive = a.avant_utm if moteur == "utm" else a.avant_qemu
        elif moteur == "utm":
            archive = a.candidat / ("utm-sonde" if sonde else "utm")
        else:
            archive = a.candidat / ("sonde" if sonde else "sans")
        cible = "aarch64-unknown-none-softfloat" if arch == "aarch64" else "x86_64-unknown-none"
        elf = archive / "novaOS/target" / cible / "release/novaos"
        if elf not in sha:
            sha[elf] = empreinte(elf)
        nom = f"{plateforme}-{secteur}-{cas}-{mode}-{version}-r{rep}"
        dossier = a.sortie / nom
        mesure = dossier / "mesure.json"
        fini = dossier / "fin.json"
        if fini.exists():
            fin = json.loads(fini.read_bytes())
            if fin["code"] or empreinte(mesure) != fin["sha256_mesure"]:
                raise ValueError(f"Essai déjà enregistré en erreur ou modifié : {dossier}")
            examiner(json.loads(mesure.read_bytes()), plateforme, secteur, cas, sonde, sha[elf])
            continue
        dossier.mkdir()
        commande = [sys.executable, "-B", BANC, moteur, arch, str(secteur), cas,
                    archive, a.generateur, mesure]
        if moteur == "utm":
            commande += ["--profil-utm", a.profil_utm]
        if sonde:
            commande.append("--sonde")
        if trace:
            commande += ["--trace-macos", dossier / "instruments"]
        debut = datetime.now(timezone.utc).isoformat()
        print(debut, nom, flush=True)
        with (dossier / "execution.log").open("x") as journal:
            code = subprocess.run(commande, stdout=journal, stderr=subprocess.STDOUT).returncode
        fin = dict(debut_utc=debut, fin_utc=datetime.now(timezone.utc).isoformat(),
                   code=code, version=version, plateforme=plateforme, secteur=secteur,
                   cas=cas, repetition=rep, mode=mode, elf_sha256=sha[elf],
                   sha256_mesure=empreinte(mesure) if mesure.exists() else None)
        fini.write_text(json.dumps(fin, ensure_ascii=False, indent=2) + "\n")
        if code:
            raise RuntimeError(f"Essai en erreur, journaux conservés dans {dossier}")
        examiner(json.loads(mesure.read_bytes()), plateforme, secteur, cas, sonde, sha[elf])


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for champ in ("avant_qemu", "avant_utm", "candidat", "generateur", "profil_utm", "sortie"):
        p.add_argument(champ, type=Path)
    p.add_argument("--repetitions", type=int, default=3,
                   help="passages hors sonde du candidat par cellule (défaut 3)")
    a = p.parse_args()
    if a.repetitions < 1:
        p.error("au moins un passage est nécessaire")
    lancer(a)
