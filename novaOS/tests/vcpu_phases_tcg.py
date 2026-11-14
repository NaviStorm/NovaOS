"""Mesurer les instructions invitées et le CPU du thread TCG entre marqueurs du prototype."""
import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path


def symboles(elf):
    texte = subprocess.check_output(["llvm-nm", "-S", "-C", elf], text=True)
    def symbole(nom):
        motif = rf"^([0-9a-f]+) ([0-9a-f]+) [bt] novaos::mesures_save::{nom}(?: |$)"
        (adresse, taille), = re.findall(motif, texte, re.M)
        return adresse, taille
    debut, taille = symbole("marker")
    return [debut, taille, *(symbole(nom)[0] for nom in ("ACTIVE", "ENTREE_MOTEUR", "DUREE_MOTEUR", "BARRIERES"))]


def lire_trace(trace, instructions):
    # Chaque point possède le décompte d'instructions dispatchées, le temps
    # CPU consommé par le seul thread de ce vCPU et l'horloge réelle du Mac.
    ouverts, sommes, bar = {}, {}, []
    def delta(fin, debut):
        d = [b - a for a, b in zip(debut, fin, strict=True)]
        assert all(x >= 0 for x in d)
        return d
    noms = ["sha_source", "sha_candidat", "texte", "reservation_pages", "construction_extents",
            "copies", "allocation_fenetres", "recherche_extents", "effacement"]
    lignes = trace.read_text().splitlines()
    assert lignes[0].startswith("S 0 ") and lignes[-1].startswith("T 0 ")
    total = None
    for ligne in lignes:
        type_, pos, *mesures = ligne.split()
        i = int(pos)
        point = list(map(int, mesures))
        if type_ == "S":
            assert len(ouverts) == 0 and total is None
            total = point
        elif type_ == "T":
            assert total is not None and not ouverts
            sommes["publication"] = delta(point, total)
        elif type_ == "E":
            assert i not in ouverts
            ouverts[i] = point
        elif type_ == "F":
            cle = noms[i]
            intervalle = delta(point, ouverts.pop(i))
            precedent = sommes.get(cle, [0, 0, 0])
            sommes[cle] = [a + b for a, b in zip(precedent, intervalle, strict=True)]
        elif type_ == "B":
            assert i == len(bar)
            bar.append(point)
        else:
            raise ValueError(ligne)
    assert len(bar) == 4 and len(sommes) == len(noms) + 1
    sommes.update(F1=delta(bar[1], bar[0]), F2=delta(bar[3], bar[2]))
    return {nom: dict(instructions=n if instructions else None, cpu_thread_ns=cpu, temps_ecoule_ns=wall)
            for nom, (n, cpu, wall) in sommes.items()}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=Path, help="archive QEMU avec mesures de sauvegarde")
    p.add_argument("plugin", type=Path)
    p.add_argument("generateur", type=Path)
    p.add_argument("architecture", choices=("aarch64", "x86_64"))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("cas", choices=("inserer-debut", "inserer-milieu", "ajout-4m-milieu", "ajout-4m-multiple"))
    p.add_argument("sortie", type=Path)
    p.add_argument("--compter", action="store_true", help="ajouter le décompte à chaque instruction invitée")
    args = p.parse_args()
    args.sortie.mkdir()
    osdir = args.archive.resolve() / "novaOS"
    sys.path.insert(0, str(osdir / "tests"))
    import cli
    from es_groupees_elargies import essai
    cible = "aarch64-unknown-none-softfloat" if args.architecture == "aarch64" else "x86_64-unknown-none"
    elf = osdir / "target" / cible / "release/novaos"
    trace = args.sortie.resolve() / "points.txt"
    elements = [str(args.plugin.resolve()), *("addr=" + n for n in symboles(elf)), "out=" + str(trace),
                "instructions=" + ("on" if args.compter else "off")]
    if any("," in champ for champ in elements):
        p.error("La syntaxe QEMU réserve la virgule dans les paramètres du plugin")
    machine = cli.machine
    def avec_plugin(*a, **kw):
        cmd, env = machine(*a, **kw)
        return [*cmd, "-plugin", ",".join(elements)], env
    cli.machine = avec_plugin
    with tempfile.TemporaryDirectory() as tmp:
        mesure = essai(args.architecture, args.secteur, args.cas, args.generateur.resolve(), Path(tmp))
    r = dict(architecture=args.architecture, secteur=args.secteur, cas=args.cas,
             sha256_elf=hashlib.sha256(elf.read_bytes()).hexdigest(), mesure=mesure, phases=lire_trace(trace, args.compter))
    (args.sortie / "resultats.json").write_text(json.dumps(r, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
