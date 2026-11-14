"""Relecture indépendante des pages appartenant aux deux profils de retrait EDU."""
import hashlib
import re
import struct


def private_pages(context, read_table, service):
    sid = 0x30
    ptr, attributes = struct.unpack_from("<QQ", context, sid * 16)
    if (attributes != 0x102 or ptr & 4095 != 1 or any(context[:sid * 16])
            or any(context[(sid + 1) * 16:])):
        raise ValueError("Contexte privé inattendu pendant le retrait incertain")
    table_pa = ptr & ~4095
    source = 0x02010000  # Deuxième génération, la première préparation est annulée.
    visited = set(service)
    for shift in (39, 30, 21, 12):
        if table_pa in visited:
            raise ValueError("Table secondaire dupliquée")
        visited.add(table_pa)
        entries = struct.unpack("<512Q", read_table(table_pa, f"niveau-{shift}.bin"))
        slot = (source >> shift) & 511
        count = 2 if shift == 12 else 1
        if any(entries[:slot]) or any(entries[slot + count:]):
            raise ValueError("Traduction secondaire supplémentaire")
        if shift != 12:
            entry = entries[slot]
            if entry & 4095 != 3:
                raise ValueError("Pointeur secondaire non RW")
            table_pa = entry & ~4095
    pages = []
    for index, (rights, byte) in enumerate(((1, 0x73), (2, 0x8c))):
        entry = entries[slot + index]
        pa = entry & ~4095
        if entry & 4095 != rights or pa in visited:
            raise ValueError("Cadre client distinct ou permission incorrects")
        visited.add(pa)
        page = read_table(pa, f"client-{index}.bin")
        if page != bytes([byte]) * 4096:
            raise ValueError("Contenu de page cliente altéré")
        pages.append({"iova": source + index * 4096, "permission": rights,
                      "motif": byte, "sha256": hashlib.sha256(page).hexdigest()})
    return pages


def check_run(serial, trace, profile):
    transfers = re.findall(r"^vtd_dmar_translate dev 00:06\.00 iova (0x[0-9a-f]+)", trace, re.M)
    lost = profile == "fin-incertaine"
    if lost:
        marker = "VTD fin EDU non confirmee BME=0 cadres_clients=2 cadres_service=8 admissions=0"
        if {int(value, 16) & ~4095 for value in transfers} != {0x02010000}:
            raise ValueError("Lecture attendue sur l'unique source de la réservation")
    else:
        marker = "VTD jamais soumise doorbells=0 cadres_clients=0"
        if transfers:
            raise ValueError("Une réservation annulée a produit une traduction DMA")
    if marker not in serial or "vtd_dmar_fault " in trace or "vtd_irq_generate " in trace:
        raise ValueError("Bilan de retrait EDU ou événement de faute inattendu")
    return {"cadres_clients": 2 if lost else 0, "retours_traduction": len(transfers),
            "retraits_acquittes": 0 if lost else 1}
