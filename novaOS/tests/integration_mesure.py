"""Rejoue une sauvegarde avec un noyau de qualification polling déjà construit."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from es_groupees_utm import essai as essai_utm
from qos_utm import Observation, verifier as verifier_utm


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("environnement", choices=("qemu", "utm"))
    p.add_argument("architecture", choices=("aarch64", "x86_64"))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("cas", choices=("neuf-4m", "inserer-milieu", "ajout-4m-multiple"))
    p.add_argument("archive", type=Path, help="export contenant novaOS/ et le noyau à mesurer")
    p.add_argument("generateur", type=Path, help="exécutable es-groupees --image")
    p.add_argument("rapport", type=Path)
    p.add_argument("--profil-utm", type=Path, help="paquet modèle UTM système de qualification")
    p.add_argument("--sonde", action="store_true", help="le noyau sélectionné contient la sonde")
    p.add_argument("--trace-macos", type=Path, metavar="DOSSIER_NEUF",
                   help="conserver la trace Instruments du vCPU et la relecture du QCOW2")
    a = p.parse_args()
    if (a.environnement == "utm") != (a.profil_utm is not None):
        p.error("--profil-utm est requis uniquement pour UTM")
    if a.environnement == "utm" and a.architecture != "aarch64":
        p.error("le profil système UTM concerne AArch64")
    if a.trace_macos and (a.environnement != "utm" or a.trace_macos.exists()):
        p.error("--trace-macos exige UTM et un nouveau répertoire")
    cible = "aarch64-unknown-none-softfloat" if a.architecture == "aarch64" else "x86_64-unknown-none"
    arbre = a.archive.resolve() / "novaOS"
    elf = arbre / "target" / cible / "release/novaos"
    generateur = a.generateur.resolve()
    somme = hashlib.sha256(elf.read_bytes()).hexdigest()
    charge = os.getloadavg()
    with tempfile.TemporaryDirectory(prefix="novaos-integration-mesure-") as tmp:
        d = Path(tmp)
        if a.environnement == "utm":
            machine = a.trace_macos.resolve() if a.trace_macos else d / "machine"
            m = essai_utm(a.profil_utm.resolve(), elf, generateur, a.secteur, machine,
                          a.sonde, False, a.cas, qos=Observation("ordinaire", "repos", bool(a.trace_macos)))
        else:
            fichier = d / "resultat.json"
            cmd = [sys.executable, "-B", arbre / "tests/es_groupees_elargies.py",
                   a.architecture, str(a.secteur), a.cas, generateur, fichier]
            if not a.sonde:
                cmd.append("--sans-sonde")
            subprocess.run(cmd, check=True)
            m = json.loads(fichier.read_bytes())
    resultat = dict(plateforme=f"{a.environnement}-{a.architecture}", elf_sha256=somme,
                    loadavg_avant=charge, loadavg_apres=os.getloadavg(), mesure=m)
    with a.rapport.open("x", encoding="utf-8") as f:
        json.dump(resultat, f, indent=2, ensure_ascii=False, allow_nan=False)
        f.write("\n")
    # Garder également les compteurs d'une sauvegarde qui dépasse un critère.
    if a.environnement == "utm":
        verifier_utm(m, dict(cas=a.cas, secteur=a.secteur, sonde=a.sonde,
                             politique="ordinaire", charge="repos"))
    if a.sonde and (m["compteur"]["bloc"]["flush"]["nombre"] != 2
                    or m["compteur"]["total_avec_controles_locaux"] > 64):
        raise ValueError("Comptage de la publication hors contrat")
    if not m.get("sha256_destination") or m["apres"][-1] <= m["avant"][-1]:
        raise ValueError("Preuve de relecture absente")
    if a.trace_macos and "instruments" not in m["qos"]:
        raise ValueError(m["qos"].get("erreur_instruments", "Capture Instruments absente"))


if __name__ == "__main__":
    main()
