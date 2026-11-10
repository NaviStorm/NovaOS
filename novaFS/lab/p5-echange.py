#!/usr/bin/env python3
"""Échanger les pools écrits par les deux exécutables P5 et contrôler les données des branches."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

LAB = Path(__file__).resolve().parent
ARCHS = {"aarch64": 0x0100000C, "x86_64": 0x01000007}


def binary(arch, test, runner=LAB / "run"):
    env = dict(os.environ, NOVAFS_TARGET=f"{arch}-apple-darwin")
    proc = subprocess.run([runner, "test", "--offline", "--no-run", "--test", test,
                           "--message-format=json"], check=True, capture_output=True, env=env)
    [path] = [m["executable"] for line in proc.stdout.splitlines()
              if (m := json.loads(line)).get("executable") and m["target"]["name"] == test]
    with open(path, "rb") as f:
        assert f.read(8) == bytes.fromhex("cffaedfe") + ARCHS[arch].to_bytes(4, "little")
    return path


def run(path, action, sector, image, test="echange_de_pool"):
    env = dict(os.environ, NOVAFS_P5_ECHANGE=action, NOVAFS_P5_SECTEUR=str(sector),
               NOVAFS_P5_IMAGE=str(image))
    subprocess.run([path, "--exact", test], env=env, check=True,
                   stdout=subprocess.DEVNULL)


def digest(image):
    with image.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def main():
    executables = {arch: binary(arch, "p5_echange") for arch in ARCHS}
    reads = 0
    refus = 0
    with tempfile.TemporaryDirectory(prefix="novafs-p5-images-") as tmp:
        old = Path(tmp) / "historique"
        old.mkdir()
        archive = old / "source.tar"
        with archive.open("wb") as output:
            subprocess.run(["git", "archive", "b865c58", "novaFS"],
                           cwd=LAB.parent.parent, stdout=output, check=True)
        subprocess.run(["tar", "-xf", archive, "-C", old], check=True)
        shutil.copyfile(LAB / "p5_ancien_ecrivain.rs", old / "novaFS/tests/ecrivain_historique.rs")
        historical = {a: binary(a, "ecrivain_historique", old / "novaFS/lab/run") for a in ARCHS}
        checksums = set()
        for arch, writer in executables.items():
            other, = (b for a, b in executables.items() if a != arch)
            for sector in (512, 4096):
                image = Path(tmp) / f"{arch}-{sector}.raw"
                run(writer, "creer", sector, image)
                assert image.stat().st_size == 8192 * 4096
                before = digest(image)
                checksums.add(before)
                for read_sector in (512, 4096):
                    run(other, "lire", read_sector, image)
                    assert digest(image) == before
                    reads += 1
                for probe in historical.values():
                    run(probe, "refuser", sector, image, "refus_du_pool_p5_avant_ecriture")
                    assert digest(image) == before
                    refus += 1
        # La géométrie de secteur s'applique aux E/S et tous les IDs de la fixture sont fixés.
        assert len(checksums) == 1, checksums
    print(json.dumps({"images": 4, "lectures_croisees": reads, "refus_ecrivains_P0": refus,
                      "sha256": checksums.pop()}))


if __name__ == "__main__":
    main()
