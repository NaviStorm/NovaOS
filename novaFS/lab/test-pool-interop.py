#!/usr/bin/env python3
"""Vérifie entre deux versions le catalogue épais et les 16 systèmes de fichiers inclus."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
CPUS = {"aarch64": 0x0100000C, "x86_64": 0x01000007}
BLOCKS = 4352


def build(launcher, arch, directory, args, name):
    env = dict(os.environ, NOVAFS_TARGET=f"{arch}-apple-darwin", CARGO_TARGET_DIR=str(directory))
    lines = subprocess.check_output(
        [launcher, *args, "--offline", "--message-format=json"], env=env, text=True
    )
    binary, = [Path(message["executable"]) for message in map(json.loads, lines.splitlines())
               if message.get("executable") and message["target"]["name"] == name]
    with binary.open("rb") as file:
        if file.read(8) != bytes.fromhex("cffaedfe") + CPUS[arch].to_bytes(4, "little"):
            raise ValueError(f"architecture inattendue de {binary}")
    return binary


def report(cli, command, sector, image):
    with image.open("rb") as file:
        output = subprocess.check_output([cli, command, str(BLOCKS), str(sector)], stdin=file)
    result = json.loads(output)
    if result["statut"] != "coherent" or len(result["volumes"]) != 16:
        raise ValueError("diagnostic incomplet du pool")
    return result


def check_accounting(result):
    expected = {
        "capacite_blocs": BLOCKS, "grain_entete_blocs": 256, "attribues_blocs": 4096,
        "disponibles_blocs": 0, "queue_blocs": 0, "differes_blocs": 0,
        "reserves_transaction_blocs": 0, "partages_entre_lv_blocs": 0, "tickets": 0,
    }
    if any(result[k] != v for k, v in expected.items()):
        raise ValueError("partition incorrecte du pool plein")
    for i, volume in enumerate(result["volumes"], start=1):
        if (volume["lv"] != (bytes([i]) * 16).hex() or volume["garantis_blocs"] != 256
                or volume["fs"]["logique_octets"] != "4096"
                or volume["fs"]["quota_logique_octets"] != 256 * 4096):
            raise ValueError(f"comptes erronés du LV {i}")


def campaign(reference, directory):
    binaries, images = {}, {}
    for version, launcher in (("reference", reference), ("courante", HERE / "run")):
        for arch in CPUS:
            cargo = directory / version
            writer = build(launcher, arch, cargo, ["test", "--no-run", "--test", "volumes"], "volumes")
            binaries[version, arch] = build(launcher, arch, cargo, ["build", "--bin", "novafs"], "novafs")
            image = directory / f"{version}-{arch}.raw"
            subprocess.run([writer, "p2_pool_image_de_mesure", "--exact", "--ignored"], check=True,
                           env=dict(os.environ, NOVAFS_POOL_FIXTURE=str(image)), stdout=subprocess.PIPE)
            if image.stat().st_size != BLOCKS * 4096:
                raise ValueError(f"longueur incorrecte : {image}")
            images[version, arch] = image

    baseline = next(iter(images.values()))
    with baseline.open("rb") as f:
        digest = hashlib.file_digest(f, "sha256").hexdigest()
    integrity, accounts = 0, 0
    for (_, arch), image in images.items():
        subprocess.run(["/usr/bin/cmp", "-s", baseline, image], check=True)
        other = next(a for a in CPUS if a != arch)
        for sector in (512, 4096):
            for version in ("reference", "courante"):
                result = report(binaries[version, other], "check-pool", sector, image)
                if result["paire"]["generation"] != 33 or any(
                    v["contenu"]["etat"] != "verifie" or v["type"] != "pret"
                    for v in result["volumes"]
                ):
                    raise ValueError("catalogue ou contenu d'un LV non vérifié")
                integrity += 1
            check_accounting(report(binaries["courante", other], "space-pool", sector, image))
            accounts += 1
        with image.open("rb") as f:
            if hashlib.file_digest(f, "sha256").hexdigest() != digest:
                raise ValueError("image modifiée pendant le contrôle")
    print(f"{len(images)} images identiques, {baseline.stat().st_size} octets, SHA-256 {digest}")
    print(f"{integrity} contrôles d'intégrité et {accounts} contrôles d'espace interarchitectures réussis")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path, help="lanceur lab/run de P2 ou d’une version ultérieure")
    with tempfile.TemporaryDirectory(prefix="novafs-pool-interop-") as temp:
        campaign(parser.parse_args().reference.resolve(), Path(temp))
