"""Exercer le décodage de vi sur Darwin et les droits IPC dans les deux invités QEMU."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def empreinte(fichier):
    with fichier.open("rb") as flux:
        return hashlib.file_digest(flux, "sha256").hexdigest()


def qualifier(archive, sortie):
    tests = Path(__file__).resolve().parent
    resultat = {"sources": {}, "essais": []}
    with tempfile.TemporaryDirectory(prefix="novaos-decouverte-") as chemin:
        travail = Path(chemin)
        trames = travail / "trames"
        trames.mkdir()
        for source in (tests / "decouverte_trames.rs", archive / "novaOS/abi/block.rs",
                       archive / "novaOS/abi/files.rs", archive / "novaOS/abi/native.rs",
                       archive / "components/libraries/vi_wire.rs"):
            shutil.copyfile(source, trames / source.name)
            resultat["sources"][source.name] = empreinte(source)
        for arch in ("aarch64", "x86_64"):
            exe = trames / arch
            subprocess.run(["rustup", "run", "stable", "rustc", "--edition=2024", "--test",
                            "--target", arch + "-apple-darwin", trames / "decouverte_trames.rs",
                            "-o", exe], check=True)
            log = subprocess.check_output([exe, "--test-threads=1"], text=True)
            if "5 passed; 0 failed" not in log:
                raise ValueError(log)
            resultat["essais"].append(dict(architecture=arch, environnement="Darwin",
                                           sha256_executable=empreinte(exe), sortie=log.splitlines()))
        # Les binaires d'origine restent disponibles pour les mesures de performances.
        copie = travail / "invite"
        subprocess.run(["cp", "-cR", archive, copie], check=True)
        fixture = tests / "fixtures/io-0166-decouverte-droits.patch"
        resultat["sources"][fixture.name] = empreinte(fixture)
        with fixture.open("rb") as f:
            subprocess.run(["patch", "-F0", "-p1", "--batch", "--forward"],
                           stdin=f, cwd=copie, check=True)
        for arch, cible in (("aarch64", "aarch64-unknown-none-softfloat"),
                            ("x86_64", "x86_64-unknown-none")):
            log = subprocess.check_output(["make", "-C", copie / "novaOS", "ARCH=" + arch, "ipc-test"],
                                          env={**os.environ, "CARGO_NET_OFFLINE": "true"},
                                          text=True, stderr=subprocess.STDOUT)
            marque = "OK — " + arch + "-ipc-test-256"
            if marque not in log.splitlines():
                raise ValueError(log)
            noyau = copie / "novaOS/target" / cible / "release/novaos"
            resultat["essais"].append(dict(architecture=arch, environnement="QEMU",
                                           sha256_elf=empreinte(noyau), sortie=marque))
    with sortie.open("x") as f:
        json.dump(resultat, f, ensure_ascii=False, indent=2)
        f.write("\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=Path, help="profil groupé reconstruit sans sonde")
    p.add_argument("sortie", type=Path)
    args = p.parse_args()
    qualifier(args.archive.resolve(), args.sortie)
