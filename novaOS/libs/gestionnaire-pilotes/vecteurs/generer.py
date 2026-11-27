"""Vecteurs DATA du profil 4C. Générateur indépendant de l'encodeur Rust."""
import struct
from pathlib import Path

root = Path(__file__).resolve().parent
epoch = 0x0102030405060708
def header(length, flags):
    return struct.pack("<HHHHQQQ", 1, 11, length, flags, epoch, 13, 7)

(root/"reserver.data").write_bytes(header(64, 0) + struct.pack("<QQQI4x", 0, 17, 42, 3))
(root/"confirme.data").write_bytes(header(88, 1) + struct.pack("<QQQ32x", 1, 8, 1))
