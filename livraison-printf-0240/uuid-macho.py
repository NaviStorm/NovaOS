"""Attribuer un UUID de contenu au Mach-O ARM64 avant sa signature ad hoc."""

import hashlib
import struct
import sys
from pathlib import Path


def update(path):
    image = bytearray(path.read_bytes())
    magic, cpu, _, kind, count, size = struct.unpack_from("<6I", image)
    if (magic, cpu, kind) != (0xFEEDFACF, 0x0100000C, 2):
        raise ValueError("Exécutable Mach-O ARM64 attendu")
    end = 32 + size
    cursor = 32
    uuid = None
    signature = None
    for _ in range(count):
        cmd, length = struct.unpack_from("<II", image, cursor)
        if length < 8 or cursor + length > end:
            raise ValueError("Commande de chargement invalide")
        if cmd == 0x1B and length == 24 and uuid is None:
            uuid = cursor + 8
        if cmd == 0x1D and length == 16 and signature is None:
            signature, signed_size = struct.unpack_from("<II", image, cursor + 8)
            if signature + signed_size != len(image):
                raise ValueError("Position de la signature inattendue")
        cursor += length
    if cursor != end or uuid is None or signature is None or signature < end:
        raise ValueError("UUID ou signature absent")
    image[uuid : uuid + 16] = bytes(16)
    # UUID version 8 réservé aux contenus propres à l'application (RFC 9562).
    identity = bytearray(hashlib.sha256(memoryview(image)[:signature]).digest()[:16])
    identity[6] = 0x80 | (identity[6] & 0x0F)
    identity[8] = 0x80 | (identity[8] & 0x3F)
    image[uuid : uuid + 16] = identity
    path.write_bytes(image)


if __name__ == "__main__":
    update(Path(sys.argv[1]))
