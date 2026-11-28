"""Autorité de laboratoire éphémère. Un bail appartient à une instance froide de VM."""

from hashlib import sha256
import secrets
import struct
from cryptography.hazmat.primitives.serialization import load_pem_private_key
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from pathlib import Path
import stat

ORIGINE = b"NovaOS/5M/OVMF-5J/ramdisk/EFI/BOOT/BOOTX64.EFI\0"
MAGIC = b"NOVA5M-TRUST-V1!"
TAILLE = 176


def cle_autorisee(fichier):
    if (
        fichier.is_symlink()
        or not stat.S_ISREG(fichier.stat().st_mode)
        or fichier.stat().st_mode & 0o077
    ):
        raise ValueError("Fichier de signature non privé")
    cle = load_pem_private_key(fichier.read_bytes(), password=None)
    if (
        not isinstance(cle, Ed25519PrivateKey)
        or cle.public_key().public_bytes_raw()
        != Path(__file__).with_name("autorite-laboratoire.pub").read_bytes()
    ):
        raise ValueError("Autorité de reçu inconnue")
    return cle


class Bail:
    def __init__(self, mode, empreinte, position, taille, cle):
        if (
            mode not in ("actif", "inactif")
            or len(empreinte) != 32
            or not 0 < position < 8 << 20
            or not 512 <= taille <= 8 << 20
        ):
            raise ValueError("Support ou politique inconnus")
        self.epoque = secrets.token_bytes(16)
        self.corps = (
            MAGIC
            + bytes((int(mode == "actif"), 0, 255, 1))
            + struct.pack("<III", position, taille, 0)
            + self.epoque
            + empreinte
            + cle.public_key().public_bytes_raw()
        )
        self.ancre = sha256(ORIGINE + self.corps).digest()
        self.recu = self.corps + cle.sign(self.corps)
        self.consomme = False

    def prendre(self, ancre):
        if self.consomme or ancre != self.ancre:
            raise ValueError("Bail périmé, divergent ou consommé")
        self.consomme = True
        return self.ancre
