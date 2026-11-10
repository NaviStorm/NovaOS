#!/usr/bin/env python3
"""F01 sur macOS. Échange de l’image entière par des pipes, sans chemin d’image."""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

HERE = Path(__file__).resolve().parent
GEOMETRIES = (512, 4096)
TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin")
START = b"\0NOVAFS-F01-DEBUT\xff"
END = b"\0NOVAFS-F01-FIN\xff"
IMAGE_BYTES = 384 * 4096


def executable(target):
    output = subprocess.check_output(
        [HERE / "lab", "test", "--no-run", "--test", "interop", "--message-format=json"],
        env={**os.environ, "NOVAFS_TARGET": target}, text=True,
    )
    paths = [msg["executable"] for line in output.splitlines()
             if (msg := json.loads(line)).get("executable") and msg["target"]["name"] == "interop"]
    if len(paths) != 1:
        raise RuntimeError(f"Exécutable de test introuvable pour {target}")
    binary = paths[0]
    # Vérifier la machine inscrite dans l’en-tête Mach-O avant de lancer le test.
    with open(binary, "rb") as stream:
        header = stream.read(8)
    cpu = {"aarch64-apple-darwin": 0x0100000C, "x86_64-apple-darwin": 0x01000007}[target]
    if header != bytes.fromhex("cffaedfe") + cpu.to_bytes(4, "little"):
        raise RuntimeError(f"Type Mach-O inattendu pour {target}")
    return binary


def run(binary, role, sector, payload=b""):
    return subprocess.check_output(
        [binary, "f01_pipe_transport", "--exact", "--nocapture", "--test-threads=1"],
        env={**os.environ, "NOVAFS_TEST_PIPE": role, "NOVAFS_TEST_SECTOR": str(sector)},
        input=payload, timeout=90,
    )


def extract(raw):
    pos = raw.index(START) + len(START)
    if raw[pos + IMAGE_BYTES:pos + IMAGE_BYTES + len(END)] != END:
        raise RuntimeError("Flux F01 incomplet")
    return raw[pos:pos + IMAGE_BYTES]


def main():
    if platform.system() != "Darwin":
        raise RuntimeError("macOS requis")
    binaries = {target: executable(target) for target in TARGETS}
    reference = None
    for source, destination in (TARGETS, TARGETS[::-1]):
        for written in GEOMETRIES:
            payload = extract(run(binaries[source], "export", written))
            if reference is None:
                reference = payload
            elif reference != payload:
                raise RuntimeError("Les encodeurs ne produisent pas la même image")
            for read in GEOMETRIES:
                run(binaries[destination], "import", read, payload)
                print(f"F01 {source} secteur={written} → {destination} secteur={read} : montage RO, lecture, validation")
    print(f"Quatre images identiques, {len(reference)} octets, SHA-256 {hashlib.sha256(reference).hexdigest()}")


if __name__ == "__main__":
    main()
