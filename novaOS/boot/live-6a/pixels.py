"""Contre-lecture du scanout QEMU avec la grille publique font8x8 0.3.1."""
from pathlib import Path
import re
import struct
import zlib

FONT = bytes.fromhex(Path(__file__).with_name("ascii8.hex").read_text())
TEXTE = ["NovaOS - Diagnostic noyau", "1  Entree noyau", "2  Memoire initialisee",
         "3  Ordonnanceur actif", "4  SMP verifie", "VERDICT OK - arret local"]


def lire(data):
    m = re.match(rb"P6\s+(\d+)\s+(\d+)\s+255\r?\n", data)
    if not m:
        raise ValueError("Capture QMP PPM invalide")
    w, h = map(int, m.groups())
    rgb = data[m.end():]
    if not (0 < w <= 8192 and 0 < h <= 8192 and len(rgb) == w * h * 3):
        raise ValueError("Géométrie PPM invalide")
    return w, h, rgb


def verifier(data, lignes):
    w, h, rgb = lire(data)
    z = min(3, w // 256, h // 88)
    if not z:
        raise ValueError("Écran trop petit")
    largeur = 256 * z
    fond, encre = bytes((16, 24, 39)), bytes((240, 244, 248))
    panneau = [bytearray(fond * largeur) for _ in range(88 * z)]
    for i, texte in enumerate(lignes):
        for j, c in enumerate(texte.encode("ascii")):
            for dy, bits in enumerate(FONT[c * 8:c * 8 + 8]):
                for dx in range(8):
                    if bits & (1 << dx):
                        x = (4 + j * 8 + dx) * z
                        y = (4 + i * 13 + dy) * z
                        for yy in range(y, y + z):
                            panneau[yy][3*x:3*(x+z)] = encre * z
    for y, row in enumerate(panneau):
        if rgb[3*y*w:3*(y*w+largeur)] != row:
            raise ValueError(f"Pixels différents sur la ligne {y}")
    return [w, h, largeur * len(panneau)]


def png(data):
    w, h, rgb = lire(data)
    def chunk(tag, buf):
        return struct.pack(">I", len(buf)) + tag + buf + struct.pack(">I", zlib.crc32(tag + buf))
    contenu = b"".join(b"\0" + rgb[i:i+w*3] for i in range(0, len(rgb), w*3))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">2I5B", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(contenu, 9)) + chunk(b"IEND", b""))
