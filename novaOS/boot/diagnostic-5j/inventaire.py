"""Catalogue fw_cfg PIO du modèle arrêté. Lire les tables avant la première instruction."""

import struct


def lire(io, cmd):
    io.write((cmd + "\n").encode())
    ligne = io.readline().split()
    if not ligne or ligne[0] != b"OK":
        raise ValueError("Lecture PIO refusée")
    return ligne[1:]


def octets(io, taille):
    if not 0 <= taille <= 256 * 1024:
        raise ValueError("Fichier firmware hors budget")
    data = bytearray()
    while len(data) < taille:
        n = min(128, taille - len(data))
        io.write(b"inb 0x511\n" * n)
        for _ in range(n):
            ok, valeur = io.readline().split()
            if ok != b"OK":
                raise ValueError("Lecture fw_cfg partielle")
            data.append(int(valeur, 16))
    return bytes(data)


def dmar(io):
    lire(io, "outw 0x510 0x19")
    count = int.from_bytes(octets(io, 4), "big")
    if count > 256:
        raise ValueError("Répertoire hors budget")
    fichiers = {}
    for _ in range(count):
        taille, cle, reserve, nom = struct.unpack(">IHH56s", octets(io, 64))
        nom = nom.rstrip(b"\0")
        if reserve or nom in fichiers:
            raise ValueError("Entrée fw_cfg ambiguë")
        fichiers[nom] = (cle, taille)

    def contenu(nom):
        cle, taille = fichiers[nom]
        lire(io, f"outw 0x510 {cle}")
        return octets(io, taille)

    try:
        linker = contenu(b"etc/table-loader")
        tables = contenu(b"etc/acpi/tables")
    finally:
        lire(io, "outw 0x510 0")
    if len(linker) % 128:
        raise ValueError("Commandes ACPI tronquées")
    resultats = []
    for i in range(0, len(linker), 128):
        op, nom, check, start, size = struct.unpack_from("<I56sIII", linker, i)
        if (
            op != 3
            or nom.rstrip(b"\0") != b"etc/acpi/tables"
            or tables[start : start + 4] != b"DMAR"
        ):
            continue
        table = tables[start : start + size]
        if (
            len(table) != size
            or size < 48
            or check != start + 9
            or struct.unpack_from("<I", table, 4)[0] != size
            or table[8] != 1
        ):
            raise ValueError("DMAR pré-liaison malformée")
        resultats.append(table)
    if len(resultats) != 1:
        raise ValueError("DMAR absente ou multiple")
    return resultats[0]


def couverture(table, base, bus):
    """Profil Q35 direct et sans ATS. Aucun RMRR ni portée de pont implicite."""
    pos = 48
    scopes = set()
    ioapics = set()
    unites = 0
    while pos < len(table):
        typ, taille = struct.unpack_from("<HH", table, pos)
        if taille < 16 or pos + taille > len(table) or typ != 0:
            raise ValueError("Réservation RMRR ou structure non couverte")
        flags, _, segment, reg = struct.unpack_from("<BBHQ", table, pos + 4)
        if flags or segment != 0 or reg != base:
            raise ValueError("Attribution DRHD incertaine")
        unites += 1
        j = pos + 16
        while j < pos + taille:
            kind, length, reserved, enum, start, slot, fun = struct.unpack_from(
                "<BBHBBBB", table, j
            )
            if (
                length != 8
                or j + length > pos + taille
                or reserved
                or slot > 31
                or fun > 7
            ):
                raise ValueError("Alias ou portée non résolue")
            if kind == 1:
                sid = (start << 8) | (slot << 3) | fun
                if enum or sid in scopes:
                    raise ValueError("Portée ambiguë")
                scopes.add(sid)
            elif kind == 3:
                sid = (start << 8) | (slot << 3) | fun
                if ioapics or sid in bus or enum != 0:
                    raise ValueError("Portée IOAPIC ambiguë")
                ioapics.add(sid)
            else:
                raise ValueError("Groupe inconnu")
            j += length
        pos += taille
    if unites != 1 or len(ioapics) != 1 or scopes != bus:
        raise ValueError(f"Portées DRHD {sorted(scopes)} et RID PCI {sorted(bus)}")
    return scopes
