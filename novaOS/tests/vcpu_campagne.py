"""Matrice UTM de latence, démarrage indépendant et relecture pour chaque observation."""
import argparse
import hashlib
import itertools
import json
from datetime import datetime, timezone
from pathlib import Path
import subprocess
import sys

PROFILS = ("repos", "cpu", "memoire", "stockage")


def matrice(hors_sonde_charge=False):
    cellules = list(itertools.product((512, 4096), (False, True), PROFILS))
    for rep in range(3):
        # Permuter le rang du profil et des deux géométries entre répétitions.
        for rang in range(len(cellules)):
            secteur, pre, charge = cellules[(5*rang + 7*rep) % len(cellules)]
            yield rep, secteur, pre, charge, "ajout-4m-multiple", True
        for charge in ("repos", "cpu"):
            for cas in ("inserer-debut", "ajout-4m-milieu"):
                yield rep, 512, False, charge, cas, True
        for pre in (False, True):
            yield rep, 4096, pre, "repos", "ajout-4m-multiple", False
        if hors_sonde_charge:
            for charge in ("cpu", "memoire", "stockage"):
                yield rep, 4096, False, charge, "ajout-4m-multiple", False


def lancer(a):
    script = Path(__file__).with_name("es_groupees_utm.py")
    a.sortie.mkdir(exist_ok=True, parents=True)
    empreintes = {elf: hashlib.sha256(elf.read_bytes()).hexdigest() for elf in (a.sonde, a.ordinaire)}
    for rep, secteur, pre, charge, cas, sonde in matrice(a.hors_sonde_charge):
        nom = f"r{rep+1}-{secteur}-{'plein' if pre else 'sparse'}-{charge}-{cas}-{'sonde' if sonde else 'normal'}"
        sortie = a.sortie / nom
        fiche = sortie/"resultats.json"
        elf = a.sonde if sonde else a.ordinaire
        empreinte = empreintes[elf]
        if fiche.is_file():
            if json.loads(fiche.read_text()).get("elf_sha256") != empreinte:
                raise RuntimeError(f"ELF divergent dans {sortie}")
            continue
        cmd = [sys.executable, "-B", script, a.profil, elf,
               a.generateur, sortie, str(secteur), "--cas", cas, "--profil-charge", charge]
        if pre:
            cmd.append("--preallocation")
        if not sonde:
            cmd.append("--sans-sonde")
        debut = datetime.now(timezone.utc).isoformat()
        with sortie.with_suffix(".log").open("x") as journal:
            subprocess.run(cmd, stdout=journal, stderr=subprocess.STDOUT, check=True)
        mesure = json.loads(fiche.read_text())
        mesure.update(elf_sha256=empreinte, repetition=rep+1, debut_utc=debut)
        definitive = sortie/"resultats-qualifies.json.tmp"
        definitive.write_text(json.dumps(mesure, ensure_ascii=False, indent=2)+"\n")
        definitive.replace(fiche)
        print(nom, mesure["sauvegarde_ms"], flush=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--hors-sonde-charge", action="store_true", help="inclure les témoins de charge sans sondes invitées")
    for nom in ("profil", "sonde", "ordinaire", "generateur", "sortie"):
        p.add_argument(nom, type=Path)
    lancer(p.parse_args())
