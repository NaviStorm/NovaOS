"""Stimulus Q35 jetable. La seule racine admise contient 512 entrées absentes."""

import socket
import struct
from acpi_fige import ram_reader, tables
from sonde import state


def description():
    body = bytes(3 * 4096)
    header = struct.pack(
        "<4sIBB6s8sI4sI",
        b"NW5W",
        36 + len(body),
        1,
        0,
        b"NOVAOS",
        b"DENYALL ",
        1,
        b"NOVA",
        1,
    )
    t = bytearray(header + body)
    t[9] = -sum(t) & 255
    return bytes(t)


def installer(q):
    # Le vCPU est suspendu sur la première instruction EFI après le chargement USB.
    # Aucun contexte hérité ne doit être remplacé. Aucun domaine ne reçoit de page.
    unites = state(q)["unites"]
    if len(unites) != 1:
        raise ValueError("Unité Q35 de test ambiguë")
    base, config = next(iter(unites.items()))
    if config["gsts"] != 0 or config["rtaddr"] != 0:
        raise ValueError("Traductions ou ressources préexistantes")
    donnees = description()
    racines = []

    def localiser(addr, table):
        if table == donnees:
            racines.append((addr + 36 + 4095) & ~4095)

    tables(q, localiser)
    if len(racines) != 1 or ram_reader(q)(racines[0], 4096) != bytes(4096):
        raise ValueError("Racine de test non attribuable à la table synthétique")
    racine = racines[0]
    base = int(base, 16)
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(2)
        sock.connect("vtd-temoin.sock")
        with sock.makefile("rwb", buffering=0) as io:
            for largeur, offset, valeur in (
                ("q", 0x20, racine),
                ("l", 0x18, 1 << 30),
                ("l", 0x18, 1 << 31),
            ):
                io.write(f"write{largeur} {base+offset} {valeur}\n".encode())
                if io.readline().strip() != b"OK":
                    raise ValueError("Échec de la programmation du témoin")
    apres = state(q)["unites"][hex(base)]
    if apres["rtaddr"] != racine or apres["gsts"] & (3 << 30) != (3 << 30):
        raise ValueError("Racine ou traduction du modèle non confirmée")
    return {
        "origine": "banc uniquement, après LoadImage, avant première instruction",
        "avant": config,
        "apres": apres,
        "racine_sans_entree": racine,
    }
