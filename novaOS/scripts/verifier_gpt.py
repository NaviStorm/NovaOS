"""Contrôle hors ligne du profil installé, sur une image brute ouverte en lecture seule."""
import struct
import uuid
import zlib

MIO = 1 << 20


def exiger(condition, element):
    if not condition:
        raise ValueError(f"Disque système refusé ({element})")


def verifier_gpt(path, s, ids):
    taille = path.stat().st_size
    exiger(s in (512, 4096) and taille % s == 0 and taille >= 64*MIO, "géométrie")
    total = taille // s
    n = 16384 // s
    exiger(len(ids) == 48, "identifiants attendus")
    with path.open("rb") as disk:
        def lire(lba, longueur):
            disk.seek(lba * s)
            data = disk.read(longueur)
            exiger(len(data) == longueur, "lecture incomplète")
            return data

        mbr = lire(0, s)
        exiger(mbr[450] == 0xee and mbr[510:512] == b"\x55\xaa" and
               struct.unpack_from("<II", mbr, 454) == (1, total-1), "MBR protecteur")
        tableaux = []
        for lba, autre, entrees in ((1, total-1, 2), (total-1, 1, total-1-n)):
            h = bytearray(lire(lba, s))
            exiger(h[:8] == b"EFI PART" and struct.unpack_from("<II", h, 8) == (65536, 92), "en-tête GPT")
            checksum, = struct.unpack_from("<I", h, 16)
            h[16:20] = bytes(4)
            exiger(zlib.crc32(h[:92]) == checksum, "CRC de l’en-tête")
            exiger(struct.unpack_from("<QQQQ", h, 24) == (lba, autre, 2+n, total-2-n), "bornes GPT")
            exiger(h[56:72] == ids[:16], "GUID du disque")
            debut, nombre, longueur, crc = struct.unpack_from("<QIII", h, 72)
            exiger((debut, nombre, longueur) == (entrees, 128, 128), "géométrie du tableau")
            table = lire(entrees, 16384)
            exiger(zlib.crc32(table) == crc, "CRC du tableau")
            tableaux.append(table)
        a, b = tableaux
        exiger(a == b and a[256:] == bytes(16384-256), "concordance des partitions")
        exiger(a[:16] == uuid.UUID("c12a7328-f81f-11d2-ba4b-00a0c93ec93b").bytes_le and
               a[128:144] == uuid.UUID("3687f202-9e5a-4df6-83db-17bb308172f9").bytes_le, "types de partition")
        exiger(a[16:32] + a[144:160] == ids[16:], "GUID des partitions")
        esp, dernier = struct.unpack_from("<QQ", a, 32)
        p9, fin = struct.unpack_from("<QQ", a, 160)
        exiger(esp == MIO//s and p9 == esp + (60 if s == 512 else 264)*MIO//s and
               dernier+1 == p9 and fin+1 == (total-1-n)//(MIO//s)*(MIO//s) and p9 < fin, "alignement")
        boot = lire(esp, s)
        exiger(boot == lire(esp+6, s) and boot[510:512] == b"\x55\xaa" and
               boot[82:90] == b"FAT32   " and struct.unpack_from("<H", boot, 11)[0] == s and
               struct.unpack_from("<I", boot, 32)[0] == p9-esp, "ESP FAT32")
        exiger(lire(p9, 8) == lire(p9+4096//s, 8) == b"NOVAP0FS", "ancres P9")
        return dict(esp=esp, p9=p9, fin_p9=fin, taille_secteur=s)
