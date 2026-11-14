"""Comparer deux pilotes de laboratoire sur QEMU direct à partir de quatre archives indépendantes."""
import argparse
from datetime import datetime, timezone
import hashlib
import itertools
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from irq_utm import lire_latences, sauver

CAS = ("ajout-4m-multiple", "inserer-milieu")
ARCHS = ("aarch64", "x86_64")
SECTEURS = (512, 4096)


def matrice(repetitions):
    """Apparier les deux modes ; permuter les cellules et alterner l’ordre des modes."""
    axes = list(itertools.product(ARCHS, SECTEURS, CAS))
    ordre = 0
    for rep in range(repetitions):
        cellules = axes[rep % len(axes):] + axes[:rep % len(axes)]
        for sonde in (False, True):
            for arch, secteur, cas in cellules:
                if rep >= 3 and (sonde or cas == "inserer-milieu"):
                    continue
                indice = axes.index((arch, secteur, cas))
                modes = ("poll", "irq") if (rep + indice + sonde) % 2 == 0 else ("irq", "poll")
                for mode in modes:
                    ordre += 1
                    yield ordre, rep+1, arch, secteur, cas, mode, sonde


def lancer(args):
    debut = datetime.now(timezone.utc).isoformat()
    out = args.sortie.resolve()
    out.mkdir(exist_ok=True)
    script = Path(__file__).resolve().parent
    lecteur = hashlib.sha256((script / "vi_ipc_sauvegarde.py").read_bytes()).digest()
    noyaux = {}
    for ordre, rep, arch, secteur, cas, mode, sonde in matrice(args.repetitions):
        profil = f"{mode}-{'sonde' if sonde else 'normal'}"
        dossier = out / f"{ordre:03d}-r{rep}-{arch}-{secteur}-{cas}-{profil}"
        fiche = dossier / "qualification.json"
        archive = args.archives.resolve() / profil
        if sonde and hashlib.sha256((archive / "novaOS/tests/vi_ipc_sauvegarde.py").read_bytes()).digest() != lecteur:
            raise ValueError(f"Utiliser le lecteur des compteurs courant dans {archive}")
        cible = "aarch64-unknown-none-softfloat" if arch == "aarch64" else "x86_64-unknown-none"
        elf = archive / "novaOS/target" / cible / "release/novaos"
        if elf not in noyaux:
            noyaux[elf] = hashlib.sha256(elf.read_bytes()).hexdigest()
        if fiche.is_file():
            resultat = json.loads(fiche.read_bytes())
            if resultat["sha256_elf"] != noyaux[elf] or resultat["ordre"] != ordre:
                raise RuntimeError(f"Résultat divergent dans {fiche}")
            continue
        instant = datetime.now(timezone.utc).isoformat()
        charge_hote = [os.getloadavg()]
        options = [arch, str(secteur), cas]
        if sonde:
            cmd = [sys.executable, "-B", script / "vcpu_phases_tcg.py", archive,
                   args.plugin.resolve(), args.generateur.resolve(), *options, dossier]
        else:
            dossier.mkdir()
            cmd = [sys.executable, "-B", archive / "novaOS/tests/es_groupees_elargies.py", *options,
                   args.generateur.resolve(), dossier / "hors-sonde.json", "--sans-sonde"]
        with dossier.with_suffix(".log").open("x") as journal:
            subprocess.run(cmd, check=True, stdout=journal, stderr=subprocess.STDOUT)
        charge_hote.append(os.getloadavg())
        if sonde:
            row = json.loads((dossier / "resultats.json").read_bytes())
            assert row["sha256_elf"] == noyaux[elf]
        else:
            row = dict(mesure=json.loads((dossier / "hors-sonde.json").read_bytes()))
        serie = archive / "novaOS/build" / f"cli-{arch}-{secteur}-elargi-{cas}.serial"
        shutil.copyfile(serie, dossier / "invité.serial")
        if sonde:
            row["requetes_bloc"] = lire_latences(serie, row["mesure"]["compteur"])
            compteur = row["mesure"]["compteur"]
            if ("attente_irq" in compteur) != (mode == "irq"):
                raise ValueError(f"Compteurs d’attente inattendus pour {mode}")
            if not (compteur["total_avec_controles_locaux"] <= 64 and compteur["bloc"]["flush"]["nombre"] == 2):
                raise ValueError("Contrat IPC ou barrières non vérifiées")
        if row["mesure"]["apres"][-1] <= row["mesure"]["avant"][-1]:
            raise ValueError("La génération n’a pas progressé après la publication")
        row.update(ordre=ordre, repetition=rep, mode=mode, sonde=sonde, architecture=arch, secteur=secteur,
                   cas=cas, sha256_elf=noyaux[elf], sha256_serie=hashlib.sha256(serie.read_bytes()).hexdigest(),
                   serie_debut_utc=debut, debut_essai_utc=instant, fin_relecture_utc=datetime.now(timezone.utc).isoformat(),
                   charge_mac_1_5_15_minutes_avant_apres_session=charge_hote)
        sauver(fiche, row)
        print(dossier.name, row["mesure"]["sauvegarde_ms"], flush=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for nom in ("archives", "plugin", "generateur", "sortie"):
        p.add_argument(nom, type=Path)
    p.add_argument("--repetitions", type=int, choices=range(3, 51), default=8)
    lancer(p.parse_args())
