"""Reproduit la frontière de profondeur des lecteurs P8 de v0.11.0 sur des images jetables."""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile

from bloc import Bench, ROOT

BLOCS = 256
DONNEES = b"Nova"


def cli(executable, secteur, commande, *args, entree, erreur=None):
    r = subprocess.run([executable, commande, str(BLOCS), str(secteur), *args],
                       input=entree, capture_output=True)
    if erreur is None:
        r.check_returncode()
    else:
        assert r.returncode == 1 and erreur in r.stderr.decode(), (r.returncode, r.stderr)
    return r.stdout


def commandes(lignes):
    return ("\n".join(lignes) + "\n").encode()


def ancres(executable, secteur, image):
    texte = cli(executable, secteur, "inspect", entree=image).decode()
    slots = re.findall(r"slot=(\d+) génération=(\d+) blocs=\d+ FS=(\w+) LV=(\w+)", texte)
    assert len(slots) == 2, texte
    return sorted(slots, key=lambda s: int(s[1]))


def arborescence(taille):
    # Allonger les noms de la feuille vers la racine. Chaque opérande reste court.
    profondeur = (taille + 255) // 256
    base, reste = divmod(taille - profondeur, profondeur)
    noms = ["z" * (base + int(i < reste)) for i in range(profondeur)]
    chemin = "/" + "/".join(noms)
    assert len(chemin) == taille and max(map(len, noms)) <= 255
    initial = "/a" * profondeur
    lignes = ["mkdir " + "/a" * i for i in range(1, profondeur)]
    lignes += ["create " + initial, f"write {initial} 0 {DONNEES.hex()}"]
    for i in reversed(range(profondeur)):
        parent = "/a" * i
        lignes.append(f"rename {parent}/a {parent}/{noms[i]}")
    return commandes(lignes), chemin


def longueur(lecteur, generateur, secteur, taille):
    source, chemin = arborescence(taille)
    profondeur = chemin.count("/")
    hors_profil = profondeur > 32
    producteur = generateur if hors_profil else lecteur
    image = cli(producteur, secteur, "script", entree=source)
    _, (slot, _, fs, lv) = ancres(lecteur, secteur, image)
    if hors_profil:
        cli(producteur, secteur, "verify", fs, lv, entree=image)
        cli(lecteur, secteur, "verify", fs, lv, entree=image, erreur="cycle ou profondeur du catalogue")
        print(f"{taille} octets, {profondeur} arêtes : refus P8 au montage RO", flush=True)
        return image
    cli(lecteur, secteur, "verify", fs, lv, entree=image)
    lecture = cli(lecteur, secteur, "salvage", fs, lv, slot, chemin, entree=image,
                  erreur="argument invalide : chemin" if taille > 1024 else None)
    assert lecture == (b"" if taille > 1024 else DONNEES)
    print(f"{taille} octets, {profondeur} arêtes : montage RO vérifié", flush=True)
    return image


def deplacement(d):
    # Le fichier de /s est à la profondeur d + 2 après déplacement.
    parent = "/x" * d
    lignes = ["mkdir " + "/x" * i for i in range(1, d + 1)]
    lignes += ["mkdir /s", "create /s/f", f"write /s/f 0 {DONNEES.hex()}",
               f"rename /s {parent}/s"]
    return commandes(lignes), parent + "/s/f"


def montage_invite(b, image, profondeur, profil, lv=None):
    b.image.write_bytes(image)
    log = b.run(profil, f"profondeur-{profondeur}-{profil}", readonly=profil == "files-info")
    if lv is None:
        assert "client=Exit(21)" in log and "BLOC session terminee ok=false" in log, log[-1400:]
    else:
        assert "BLOC session terminee ok=true" in log and f"Volume {lv}" in log, log[-1400:]
    assert b.image.read_bytes() == image
    print(f"{profondeur} arêtes, {profil} : {'monté' if lv else 'CORRUPT'}, image identique", flush=True)


def matrice(arch, secteur, generateur):
    lecteur = ROOT.parent / f"novaFS/target/{arch}-apple-darwin/release/novafs"
    print(f"hôte/invité={arch} secteur={secteur}", flush=True)
    with tempfile.TemporaryDirectory(prefix="nova-profondeur-") as tmp:
        b = Bench(Path(tmp), arch, secteur)
        for taille in (1023, 1024, 1025, 1074, 4096, 8192):
            longueur(lecteur, generateur, secteur, taille)
        longue = longueur(lecteur, generateur, secteur, 10001)
        montage_invite(b, longue, 40, "files-info")
        for profondeur in (32, 33):
            source, chemin = deplacement(profondeur - 2)
            if profondeur == 33:
                cli(lecteur, secteur, "script", entree=source, erreur="profondeur après renommage")
            producteur = lecteur if profondeur == 32 else generateur
            image = cli(producteur, secteur, "script", entree=source)
            ancienne, (slot, generation, fs, lv) = ancres(lecteur, secteur, image)
            assert int(generation) == int(ancienne[1]) + 1
            cli(producteur, secteur, "verify", fs, lv, entree=image)
            assert cli(producteur, secteur, "salvage", fs, lv, slot, chemin, entree=image) == DONNEES
            assert cli(lecteur, secteur, "salvage", fs, lv, ancienne[0], "/s/f", entree=image) == DONNEES
            cli(lecteur, secteur, "verify", fs, lv, entree=image,
                erreur="cycle ou profondeur du catalogue" if profondeur == 33 else None)
            for profil in (["files-info"] if profondeur == 32 else ["files-info", "files-write"]):
                montage_invite(b, image, profondeur, profil, lv if profondeur == 32 else None)


if __name__ == "__main__":
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("arch", choices=["aarch64", "x86_64"])
    args.add_argument("generateur", type=Path, help="CLI témoin temporaire construite avec MAX_DEPTH=40")
    opts = args.parse_args()
    for secteur in (512, 4096):
        matrice(opts.arch, secteur, opts.generateur.resolve())
