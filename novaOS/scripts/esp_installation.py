"""Construire les secteurs FAT32 du chargeur, sans ouvrir de disque cible."""
import argparse
import math
from pathlib import Path
import struct
import tempfile
from licences_service import notices
import zlib

ROOT = Path(__file__).resolve().parents[1]


def secteur_entree(nom, cluster, taille=0, dossier=False):
    entree = bytearray(32)
    entree[:11] = nom
    entree[11] = 16 if dossier else 32
    for offset in (16, 18, 24):
        struct.pack_into("<H", entree, offset, 33)  # 1er janvier 1980
    struct.pack_into("<H", entree, 20, cluster >> 16)
    struct.pack_into("<HI", entree, 26, cluster & 65535, taille)
    return entree


def nom_long(nom, court):
    checksum = 0
    for octet in court:
        checksum = (((checksum >> 1) | (checksum << 7)) + octet) & 255
    entree = bytearray(32)
    entree[0], entree[11], entree[13] = 0x41, 15, checksum
    data = (nom + "\0").encode("utf-16-le").ljust(26, b"\xff")
    entree[1:11], entree[14:26], entree[28:32] = data[:10], data[10:22], data[22:]
    return entree


def construire(elf, secteur, *, scenario="systeme-gpt"):
    if scenario not in ("systeme-gpt", "smp-test", "smp-fp", "smp-arret"):
        raise ValueError("Scénario EFI inconnu")
    taille = (60 if secteur == 512 else 264) * 2**20 // secteur
    reserve = 32
    fat = math.ceil((taille - reserve + 2) * 4 / (secteur + 8))
    debut = reserve + 2 * fat
    clusters = taille - debut
    assert clusters >= 65525 and fat * secteur >= 4 * (clusters + 2)
    kernel = elf.read_bytes()
    loader = (ROOT / "third_party/limine/BOOTAA64.EFI").read_bytes()
    config = (ROOT / "boot/limine.conf").read_bytes() + f"    cmdline: {scenario}\n".encode("ascii")
    if kernel[:6] != b"\x7fELF\x02\x01" or kernel[18:20] != b"\xb7\0" or loader[:2] != b"MZ":
        raise ValueError("composant AArch64 invalide")
    secteurs = {}
    chaines = [0xffffff8, 0xfffffff] + [0xfffffff] * 4  # racine, EFI, BOOT EFI, boot noyau

    def fichier(nom, data):
        premier = len(chaines)
        for off in range(0, len(data), secteur):
            cluster = len(chaines)
            chaines.append(cluster + 1 if off + secteur < len(data) else 0xfffffff)
            secteurs[debut + cluster - 2] = data[off:off+secteur]
        return secteur_entree(nom, premier, len(data))

    conf = b"LIMINE~1CON"
    def dossier(nom, cluster):
        return secteur_entree(nom, cluster, dossier=True)

    def liens(cluster, parent):
        return dossier(b".          ", cluster) + dossier(b"..         ", parent)

    with tempfile.TemporaryDirectory() as temp:
        notice = Path(temp) / "service.txt"
        notices("aarch64-unknown-none-softfloat", notice)
        sources = [*sorted((ROOT/"licenses").glob("*.txt")), ROOT.parent/"components/LICENSE", notice]
        licences = b"".join(p.name.encode() + b"\n" + p.read_bytes() + b"\n" for p in sources)
    secteurs[debut] = (dossier(b"EFI        ", 3) + dossier(b"BOOT       ", 5) +
                        nom_long("limine.conf", conf) + fichier(conf, config) + fichier(b"LICENCES TXT", licences))
    secteurs[debut+1] = liens(3, 0) + dossier(b"BOOT       ", 4)
    secteurs[debut+2] = liens(4, 3) + fichier(b"BOOTAA64EFI", loader)
    secteurs[debut+3] = liens(5, 0) + fichier(b"NOVAOS  ELF", kernel)
    if len(chaines) - 2 > clusters:
        raise ValueError("ESP trop petite")
    boot = bytearray(secteur)
    boot[:11] = b"\xeb\x58\x90NOVAOS  "
    struct.pack_into("<HBHBHHBHHHII", boot, 11, secteur, 1, reserve, 2, 0, 0, 0xf8,
                     0, 63, 255, 2**20 // secteur, taille)
    struct.pack_into("<IHHIHH", boot, 36, fat, 0, 0, 2, 1, 6)
    boot[64], boot[66] = 0x80, 0x29
    struct.pack_into("<I", boot, 67, zlib.crc32(kernel))
    boot[71:90] = b"NO NAME    FAT32   "
    boot[510:512] = b"\x55\xaa"
    info = bytearray(secteur)
    info[0:4], info[484:488], info[508:512] = b"RRaA", b"rrAa", b"\0\0\x55\xaa"
    struct.pack_into("<II", info, 488, clusters - len(chaines) + 2, len(chaines))
    secteurs[0] = secteurs[6] = boot
    secteurs[1] = secteurs[7] = info
    table = struct.pack(f"<{len(chaines)}I", *chaines)
    for off in range(0, len(table), secteur):
        i = reserve + off // secteur
        secteurs[i] = secteurs[i+fat] = table[off:off+secteur]
    return b"".join(struct.pack("<I", i) + bytes(b).ljust(secteur, b"\0")
                    for i,b in sorted(secteurs.items()) if any(b))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("elf", type=Path)
    p.add_argument("dossier", type=Path)
    args = p.parse_args()
    args.dossier.mkdir()
    for secteur in (512, 4096):
        (args.dossier / f"esp-{secteur}.bin").write_bytes(construire(args.elf, secteur))
