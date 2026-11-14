"""Vérifie une image P9 entre deux révisions Git, sur Darwin ARM64 et Rosetta."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

RACINE = Path(__file__).resolve().parents[3]
BANC = Path(__file__).with_suffix(".rs")
CIBLES = ("aarch64-apple-darwin", "x86_64-apple-darwin")


def empreinte(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def construire(revision, d, prototype):
    sha = subprocess.check_output(["git", "rev-parse", "--verify", revision + "^{commit}"],
                                  cwd=RACINE, text=True).strip()
    d.mkdir()
    archive = d / "sources.tar"
    subprocess.run(["git", "archive", "-o", archive, sha, "novaFS"], cwd=RACINE, check=True)
    subprocess.run(["tar", "-xf", archive, "-C", d], check=True)
    moteur = d / "novaFS"
    shutil.copyfile(BANC, moteur / "examples/interop-integration.rs")
    binaires = {}
    rustc = subprocess.check_output(["rustup", "which", "rustc"], text=True).strip()
    cargo = subprocess.check_output(["rustup", "which", "cargo"], text=True).strip()
    for cible in CIBLES:
        cmd = [cargo, "build", "--offline", "--locked", "--release", "--example",
               "interop-integration", "--target", cible]
        if prototype:
            cmd += ["--features", "prototype-es-groupees"]
        subprocess.run(cmd, cwd=moteur, check=True, env={**os.environ, "RUSTC": rustc,
                            "CARGO_PROFILE_RELEASE_STRIP": "none",
                            "RUSTFLAGS": '--check-cfg=cfg(feature,values("prototype-es-groupees"))'})
        binaires[cible] = moteur / "target" / cible / "release/examples/interop-integration"
    return sha, binaires


def qualifier(publie, candidat, image, secteur):
    lignes = []
    for exe, action in [(publie, "creer"), (candidat, "grouper"), (publie, "retour"),
                        (candidat, "lire"), (publie, "lire")]:
        generation = int(subprocess.check_output([exe, action, image, str(secteur)], text=True))
        lignes.append(dict(action=action, generation=generation, image_sha256=empreinte(image)))
    generations = [r["generation"] for r in lignes]
    if not (generations[0] < generations[1] < generations[2] == generations[3] == generations[4]):
        raise ValueError(f"Générations inattendues {generations}")
    if len({r["image_sha256"] for r in lignes[2:]}) != 1:
        raise ValueError("Montage en lecture seule ayant changé l’image")
    return lignes


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for champ in ("publie", "candidat", "rapport"):
        p.add_argument(champ)
    a = p.parse_args()
    with tempfile.TemporaryDirectory(prefix="novafs-interop-integration-") as tmp:
        d = Path(tmp)
        old, anciens = construire(a.publie, d / "publie", False)
        new, candidats = construire(a.candidat, d / "prototype", True)
        resultats = []
        for i, arch in enumerate(CIBLES):
            autre = CIBLES[1 - i]
            for secteur in (512, 4096):
                resultats.append(dict(cible_publiee=arch, cible_prototype=autre, secteur=secteur,
                                      etapes=qualifier(anciens[arch], candidats[autre],
                                                      d / f"{arch}-{secteur}.raw", secteur)))
        releve = dict(publie=old, prototype=new, banc_sha256=empreinte(BANC),
                      binaires={sha: {c: empreinte(f) for c, f in bins.items()}
                                for sha, bins in ((old, anciens), (new, candidats))},
                      essais=resultats)
        Path(a.rapport).write_text(json.dumps(releve, ensure_ascii=False, indent=2) + "\n")
