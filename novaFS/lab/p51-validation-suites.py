#!/usr/bin/env python3
"""Rejouer les suites Cargo et vérifier les effectifs d'injection P5 d'une architecture."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

LAB = Path(__file__).resolve().parent
CPU_MACHO = {"aarch64": 0x0100000C, "x86_64": 0x01000007}


def cargo(env, *args):
    cmd = [LAB / "run", "test", "--offline", *args]
    proc = subprocess.run(cmd, env=env, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, check=False)
    if proc.returncode:
        raise RuntimeError(proc.stdout[-16000:])
    return proc.stdout


def totaux(texte):
    suites = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', texte)
    if not suites:
        raise ValueError("aucun résultat du harnais Rust")
    return dict(zip(("reussis", "echecs", "ignores"), map(sum, zip(*(map(int, v) for v in suites)))))


def mesures(texte, prefixe):
    return re.findall(r'\b' + re.escape(prefixe) + r' [^\n]+', texte)


def couverture_p51(texte):
    noms = ("curseur_du_montage_cow", "rang_des_aretes_creuses_et_racines_divergentes",
            "donnees_propres_apres_f2_uniquement")
    for nom in noms:
        if not re.search(r'^test (?:\S+::)?' + re.escape(nom) + r' \.\.\. ok$', texte, re.M):
            raise ValueError(f"test P5.1 absent : {nom}")
    return list(noms)


def verifier_pannes(texte):
    lignes = mesures(texte, "PANNE")
    chiffres = [re.fullmatch(r'PANNE (\w+) frontières=(\d+) coupures=(\d+) EIO=(\d+) déchirures=(\d+)', s)
                for s in lignes]
    if len(chiffres) != 12 or not all(chiffres):
        raise ValueError(f"couverture des injections P5 incomplète : {lignes}")
    # Le test Rust boucle sur [512, 4096] et exerce six opérations par secteur.
    resultat = {}
    for i, secteur in enumerate((512, 4096)):
        lignes_secteur = [m.groups() for m in chiffres[6*i:6*i+6]]
        totaux_secteur = tuple(sum(int(c[j]) for c in lignes_secteur) for j in range(1, 5))
        if totaux_secteur != (114, 342, 76, 6):
            raise ValueError(f"frontières/P5 différentes du corpus de référence : {secteur} {totaux_secteur}")
        resultat[secteur] = [
            dict(operation=operation, frontieres=int(f), coupures=int(c), erreurs_io=int(e), dechirees=int(d))
            for operation, f, c, e, d in lignes_secteur
        ]
    return resultat


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("architecture", choices=tuple(CPU_MACHO))
    args = parser.parse_args()
    env = dict(os.environ, NOVAFS_TARGET=f"{args.architecture}-apple-darwin", CARGO_NET_OFFLINE="true")
    infos = cargo(env, "--no-run", "--test", "p5_partage", "--message-format=json")
    executables = [m["executable"] for ln in infos.splitlines() if ln.startswith("{")
                   and (m := json.loads(ln)).get("executable") and m["target"]["name"] == "p5_partage"]
    if len(executables) != 1:
        raise ValueError("binaire de contrôle absent")
    with open(executables[0], "rb") as fd:
        if fd.read(8) != bytes.fromhex("cffaedfe") + CPU_MACHO[args.architecture].to_bytes(4, "little"):
            raise ValueError("architecture du binaire incorrecte")
    standard = cargo(env, "--", "--nocapture", "--test-threads=1")
    sondes = cargo(env, "--all-features", "--", "--test-threads=1")
    modele = cargo(env, "--test", "properties", "--", "--ignored", "--nocapture")
    with tempfile.TemporaryDirectory(prefix="novafs-p51-tests-") as tmp:
        supplement = cargo(dict(env, NOVAFS_POOL_FIXTURE=str(Path(tmp) / "pool.raw")),
                           "--test", "volumes", "--test", "cout_extents", "--", "--ignored")
    diff, = mesures(modele, "DIFFERENTIEL")
    if "opérations=1000000 " not in diff:
        raise ValueError("campagne différentielle incomplète")
    r = dict(architecture=args.architecture, standard=totaux(standard), sondes=totaux(sondes),
             modele=totaux(modele), options=totaux(supplement), differentiel=diff,
             pannes_p5=verifier_pannes(standard),
             controles_p51_standard=couverture_p51(standard),
             controles_p51_sondes=couverture_p51(sondes))
    if r["options"]["reussis"] != 2 or r["modele"]["reussis"] != 1:
        raise ValueError("tests optionnels incomplets")
    for marque in ("MIGRATION", "PERMUTATIONS", "SATURATION", "ALEATOIRE", "RESTITUTION"):
        r[marque.lower()] = mesures(standard, marque)
        if not r[marque.lower()]:
            raise ValueError(f"campagne absente : {marque}")
    print(json.dumps(r, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
