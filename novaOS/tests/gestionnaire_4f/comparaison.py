"""Comparer les mêmes ELF sous Q35, en six paires de démarrages d’ordre alterné."""
import argparse
import json
from pathlib import Path

from qualifier import construire, demarrer, write


def ordre_attendu():
    for paire in range(1, 7):
        ordre = ("statique", "ethernet") if paire % 2 else ("ethernet", "statique")
        for cas in ordre:
            yield paire, cas


def verifier(dest):
    precedent = None
    demarrages = []
    for paire, cas in ordre_attendu():
        campagne = json.loads((dest/cas/str(paire)/"campagne.json").read_text())
        debut, fin = campagne["acquisition"], campagne["liberation"]
        if not campagne["succes"] or debut >= fin or (precedent and debut <= precedent):
            raise ValueError(("Ordre des démarrages ou échec du pair", paire, cas))
        precedent = fin
        demarrages.append(dict(paire=paire, profil=cas, elf=campagne["elf"],
                               debut=debut, fin=fin, revision=campagne["commit_sources"]))
    if len({b["revision"] for b in demarrages}) != 1:
        raise ValueError("Révisions différentes dans la comparaison")
    for cas in ("statique", "ethernet"):
        if len({b["elf"] for b in demarrages if b["profil"] == cas}) != 1:
            raise ValueError(("Image instable", cas))
    return demarrages


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archive_images", type=lambda s: Path(s).resolve())
    cli.add_argument("destination", type=lambda s: Path(s).resolve())
    cli.add_argument("--revision", default="HEAD")
    args = cli.parse_args()
    # Les deux images sont prêtes avant le début de la première cellule.
    images = {cas: construire(cas, args.archive_images, args.revision) for cas in ("statique", "ethernet")}
    for paire, cas in ordre_attendu():
        exe, revision = images[cas]
        demarrer(exe, revision, cas, args.destination/cas/str(paire))
    write(args.destination/"ordre.json", verifier(args.destination))


if __name__ == "__main__":
    main()
