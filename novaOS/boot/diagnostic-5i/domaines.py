"""Décodage borné d'un instantané VT-d legacy. Aucune adresse ne déclenche une lecture matérielle."""

import struct


def legacy(unit, pages, permitted_data):
    """Le producteur fournit séparément les pages de tables et les cadres de données admis."""
    if (
        unit["ver"] >> 4 != 1
        or not unit["gsts"] & 1 << 31
        or unit["rtaddr"] & 4095
        or not unit["cap"] & 1 << 10
        or unit["cap"] & 7 == 7
        or not 1 <= len(pages) <= 64
    ):
        raise ValueError("Traduction inactive, mode inconnu ou géométrie non couverte")
    mgaw = ((unit["cap"] >> 16) & 63) + 1
    if mgaw < 48 or mgaw > 64:
        raise ValueError("Largeur de traduction incompatible")
    if set(pages) & set(permitted_data) or len(permitted_data) > 4096:
        raise ValueError("Provenance non disjointe ou trop de feuilles")
    seen = set()
    leaves = set()

    def values(pa):
        if (
            pa not in pages
            or pa in seen
            or pa <= 0
            or pa & 4095
            or len(pages[pa]) != 4096
        ):
            raise ValueError("Table sans provenance, cyclique, partagée ou tronquée")
        seen.add(pa)
        return struct.unpack("<512Q", pages[pa])

    def pairs(pa):
        words = values(pa)
        for i in range(0, 512, 2):
            lo, hi = words[i : i + 2]
            if lo or hi:
                if lo & 4095 != 1:
                    raise ValueError("Entrée non présente, réservée ou pass-through")
                yield i // 2, lo & ~4095, hi

    def walk(pa, level, prefix, out):
        for i, value in enumerate(values(pa)):
            if not value:
                continue
            bits = value & 4095
            addr = value & ~4095
            iova = prefix | i << (12 + 9 * level)
            if level:
                if bits != 3:
                    raise ValueError(
                        "Superpage, droits intermédiaires ou mode non couvert"
                    )
                walk(addr, level - 1, iova, out)
            else:
                if (
                    bits not in (1, 2, 3)
                    or addr not in permitted_data
                    or addr in leaves
                ):
                    raise ValueError("Cadre de données sans preuve ou partagé")
                leaves.add(addr)
                out.append({"iova": iova, "cadre": addr, "droits": bits})

    domains = {}
    did_seen = set()
    for bus, ctx, reserved in pairs(unit["rtaddr"]):
        if reserved:
            raise ValueError("Format de racine inconnu")
        for devfn, root, hi in pairs(ctx):
            if hi & ~0xFFFF00 != 2 or (hi >> 8) >= (
                1 << min(16, 4 + 2 * (unit["cap"] & 7))
            ):
                raise ValueError("Mode de contexte ou DID inconnu")
            did = (hi >> 8) & 65535
            if did in did_seen:
                raise ValueError("DID partagé hors du profil")
            did_seen.add(did)
            sid = bus << 8 | devfn
            maps = []
            walk(root, 3, 0, maps)
            domains[sid] = {"did": did, "mappings": maps}
    if seen != set(pages):
        raise ValueError("Registre de tables incompatible avec la racine")
    return domains
