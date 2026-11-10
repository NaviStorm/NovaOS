#!/usr/bin/env python3
"""Vérifier dans les deux sens la reprise en écriture des pools et les exports P5."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

LAB = Path(__file__).resolve().parent
BASE = "ae508ce"
HISTORIQUES = ("d0ba7cb", "38c8699", "6f15350", "1510eca", "0a89d23", "b865c58", BASE)
SECTEURS = (512, 4096)
POOL = runpy.run_path(str(LAB / "p5-echange.py"))
EXPORT = runpy.run_path(str(LAB / "p5-export.py"))
CLI = runpy.run_path(str(LAB / "test-pool-interop.py"))
NB = 8192
TEMOIN = b"point de sauvegarde P5"


def verifier(condition, message):
    if not condition:
        raise ValueError(message)


def ouvrir_outils(lanceur):
    return {
        arch: (
            POOL["binary"](arch, "p5_echange", lanceur),
            CLI["build"](lanceur, arch, lanceur.parent.parent / "target",
                         ["build", "--bin", "novafs"], "novafs"),
        )
        for arch in POOL["ARCHS"]
    }


def lecture(cli, cmd, image):
    return EXPORT["run"](cli, cmd, image).stdout


def reprise(producteur, lecteur, arch, autre_arch, secteur, travail):
    """Le lecteur étranger écrit sur le clone, le producteur valide ses nouvelles pages."""
    ecrivain, cli_source = producteur[arch]
    lecteur_etranger, cli_etranger = lecteur[autre_arch]
    image = travail / "source.raw"
    POOL["run"](ecrivain, "creer", secteur, image)
    empreinte = POOL["digest"](image)
    for secteur_lecture in SECTEURS:
        POOL["run"](lecteur_etranger, "lire", secteur_lecture, image)
        verifier(POOL["digest"](image) == empreinte, "pool changé par une lecture")
    desc = json.loads(lecture(cli_etranger, ["list-views", NB, secteur], image))["vues"]
    snapshot, clone = sorted(desc, key=lambda d: d["id"])
    alteree = travail / "reprise.raw"
    contenu = bytearray(bytes(4088) + TEMOIN)
    contenu[4094:4098] = bytes((17, 18, 19, 20))
    correction = b"reprise P5/P5.1"
    contenu[4092:4092 + len(correction)] = correction
    sec = 4096 if secteur == 512 else 512
    nouveau = json.loads(EXPORT["run"](
        cli_etranger, ["file-write", NB, sec, clone["id"], "/état", 4092, correction.hex()],
        image, alteree).stderr)
    for id_vue, cible in ((snapshot["id"], bytes(4088) + TEMOIN), (clone["id"], contenu)):
        octets = lecture(cli_source, ["file-read", NB, secteur, id_vue, "/état", 0, NB], alteree)
        verifier(octets == cible, "les octets du snapshot ou du clone ont changé")
    for recu in (snapshot, nouveau):
        rapport = json.loads(lecture(cli_source, ["check", NB, secteur, recu["pool"],
                                                   recu["id"], recu["empreinte"]], alteree))
        verifier(rapport["statut"] == "coherent", "reçu du pool modifié refusé")
    EXPORT["run"](cli_source, ["check", NB, secteur, clone["pool"], clone["id"],
                                clone["empreinte"]], alteree, expected=1)
    verifier(POOL["digest"](image) == empreinte, "la source de la reprise a changé")
    return empreinte


def campagne(lanceur, work):
    versions = {"P5": ouvrir_outils(lanceur), "P5.1": ouvrir_outils(LAB / "run")}
    essais = []
    for source, outils in versions.items():
        destination = next(n for n in versions if n != source)
        autres = versions[destination]
        for arch in outils:
            cible = next(a for a in autres if a != arch)
            for secteur in SECTEURS:
                with tempfile.TemporaryDirectory(dir=work) as t:
                    dossier = Path(t)
                    h = reprise(outils, autres, arch, cible, secteur, dossier)
                    export = dossier / "archive"
                    export.mkdir()
                    sauvegarde = EXPORT["cycle"](export, outils[arch][1], autres[cible][1], secteur)
                essais.append(dict(producteur=source, architecture=arch, lecteur=destination,
                                   architecture_lecteur=cible, secteur=secteur, sha256_pool=h,
                                   lectures_geometries=2, reprises_inscriptibles=1, **sauvegarde))
    for version in versions:
        verifier(len({r["sha256_pool"] for r in essais if r["producteur"] == version}) == 1,
                 "le pool dépend de l'architecture ou de la géométrie")
        for secteur in SECTEURS:
            verifier(len({r["sha256"] for r in essais
                          if r["producteur"] == version and r["secteur"] == secteur}) == 1,
                     "l’archive dépend de l’architecture")
    return essais


def extraire(revision, work):
    racine = work / revision
    racine.mkdir()
    archive = racine / "source.tar"
    with archive.open("wb") as out:
        subprocess.run(["git", "archive", revision, "novaFS"], cwd=LAB.parent.parent,
                       stdout=out, check=True)
    subprocess.run(["tar", "-xf", archive, "-C", racine], check=True)
    return racine / "novaFS/lab/run"


def historiques(work, archives):
    """Chaque banc existant contrôle les octets, les deux lecteurs et les deux producteurs."""
    resultats = {}
    for revision in HISTORIQUES:
        lanceur = (archives / revision / "novaFS/lab/run") if archives else extraire(revision, work)
        etapes = ["test-interop.py"]
        if revision not in HISTORIQUES[:2]:
            etapes.append("test-pool-interop.py")
        resultat = {}
        for etape in etapes:
            p = subprocess.run([sys.executable, LAB / etape, "--reference", lanceur],
                               env=dict(os.environ, CARGO_NET_OFFLINE="true"), text=True,
                               stdout=subprocess.PIPE, check=True)
            resultat[etape] = p.stdout.strip().splitlines()
        resultats[revision] = resultat
    return resultats


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", choices=("partage", "historiques"), default="partage",
                        help="pools partagés P5 ou images de contenu et pools historiques")
    parser.add_argument("--archives", type=Path,
                        help="réutiliser des sources extraites par git archive, rangées par commit")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="novafs-p51-compat-") as tmp:
        work = Path(tmp)
        if args.corpus == "historiques":
            resultats = historiques(work, args.archives)
        else:
            lanceur = (args.archives / BASE / "novaFS/lab/run") if args.archives else extraire(BASE, work)
            resultats = {"reference": BASE, "echanges": campagne(lanceur.resolve(), work)}
        print(json.dumps(resultats, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
