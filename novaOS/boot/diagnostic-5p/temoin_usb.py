"""Recoupements privés du banc. Une attribution observée ne délègue aucun droit."""

import re
import struct
from observation import etendue, require


def partition(fd):
    """Première entrée du média dont les deux GPT ont été validées par image5o."""
    fd.seek(2 * 512)
    entry = fd.read(128)
    first, last = struct.unpack_from("<QQ", entry, 32)
    return ["partition", 1, first, last - first + 1, entry[16:32].hex(), 2, 2]


def chemin_usb(snapshot, origin, bds, partition_attendue, liaison):
    nodes = origin["peripherique"]
    types = [n[0] for n in nodes]
    require(
        types[:2] == ["acpi", "pci"]
        and nodes[0][2] == 0
        and types[-1:] == ["partition"]
        and nodes[-1] == partition_attendue,
        "Racine ou partition non attribuable",
    )
    suite = nodes[2:-1]
    if suite and suite[-1][0] == "lun":
        require(suite[-1][1] == 0, "LUN hors profil")
        suite = suite[:-1]
    require(
        suite and all(n[0] == "usb" and n[2] == 0 for n in suite),
        "Route ou interface USB ambiguë",
    )
    slot, fn = nodes[1][1:]
    rid = f"0000:00:{slot:02x}.{fn}"
    ctrl = snapshot["controleurs"][rid]
    require(
        liaison.endswith("/" + ctrl["qdev"] + "/" + ctrl["qdev"] + ".0"),
        "Bus QOM contradictoire",
    )
    expected = f"PciRoot(0x0)/Pci({slot:#x},{fn:#x})" + "".join(
        f"/USB({n[1] - 1:#x},{n[2]:#x})" for n in suite
    )
    require(
        bds.partition(" from ")[2].lower() == expected.lower(),
        "Chemins UEFI contradictoires",
    )
    root = suite[0][1]
    require(root in ctrl["ports"], "Port racine déconnecté")
    route = 0
    for i, node in enumerate(suite[1:]):
        require(i < 5 and 1 <= node[1] <= 15, "Route hub hors limites")
        route |= node[1] << (4 * i)
    (match,) = [
        s for s in ctrl["slots"] if s["port_racine"] == root and s["route"] == route
    ]
    require(
        match["etat"] == 3 and {e["type"] for e in match["endpoints"]} >= {2, 4, 6},
        "Endpoints de stockage insuffisants",
    )
    (protocol,) = [r for r in ctrl["protocoles"] if r[1] <= root < r[1] + r[2]]
    path = ".".join(
        str(v) for v in [root - protocol[1] + 1] + [n[1] for n in suite[1:]]
    )
    (row,) = [r for r in snapshot["bus_usb"] if r.endswith("ID: cle1")]
    require(
        f".{match['adresse_usb']}, Port {path}," in row,
        "Adresse ou port QEMU différent",
    )
    return dict(
        rid=rid,
        port=root,
        chemin_hub=path,
        slot=match["slot"],
        adresse=match["adresse_usb"],
        dci=[e["dci"] for e in match["endpoints"]],
        lun=0,
    )


def tampons(raw, spans, base, size):
    ranges = set()
    for m in re.finditer(
        rb"(?m)^usb_xhci_fetch_trb addr (0x[0-9a-f]+), (?:TR_NORMAL|TR_DATA), p (0x[0-9a-f]+), s (0x[0-9a-f]+), c (0x[0-9a-f]+)$",
        raw,
    ):
        ring, addr, status, control = (int(x, 16) for x in m.groups())
        length = status & 0x1FFFF
        if not length or control & (1 << 6):
            continue
        etendue(ring, 16, 16, spans)
        etendue(addr, length, 1, spans)
        ranges.add((addr, length))
    require(ranges, "Aucun buffer USB décrit")
    return dict(
        etendues=sorted(ranges),
        recouvrements=sum(a < base + size and a + n > base for a, n in ranges),
        provenance="TRB lus par le modèle",
        titulaire_memoire="indéterminé",
    )


def lecture_hotplug(raw, entry, halt, start, end):
    require(0 <= entry <= start < end <= halt <= len(raw), "Bornes de la reconnexion")
    inner = raw[start:end]
    require(
        b"blk_co_preadv " not in raw[entry:start] + raw[end:halt]
        and b"blk_co_pwritev " not in raw[entry:halt],
        "E/S bloc hors du branchement à vCPU arrêté",
    )
    reads = [line for line in inner.splitlines() if line.startswith(b"blk_co_preadv ")]
    require(len(reads) == 1, "Lectures du modèle inattendues")
    read = re.fullmatch(
        rb"blk_co_preadv blk (0x[0-9a-f]+) bs (0x[0-9a-f]+) offset 0 bytes 512 flags 0x0",
        reads[0],
    )
    require(read is not None, "Lecture de géométrie hors bornes")
    prefix = inner.split(reads[0], 1)[0]
    binds = re.findall(
        rb"(?m)^blk_root_attach child 0x[0-9a-f]+ blk (0x[0-9a-f]+) bs (0x[0-9a-f]+)$",
        prefix,
    )
    require((read[1], read[2]) in binds, "Lecture sans attachement au nouveau backend")
    return 512


def garde(epoque, origine, sources, changement, vol):
    require(re.fullmatch("[a-f0-9]{32}", epoque), "Époque du témoin invalide")
    if changement:
        reason = "incarnation remplacée"
    elif not sources:
        reason = "support absent"
    elif len(sources) > 1:
        reason = "plusieurs médias identiques"
    elif vol:
        reason = "requête héritée en vol"
    else:
        reason = "propriété des pages et protection avant chargement indéterminées"
    return dict(
        epoque=epoque,
        chemin=origine,
        incarnation=1,
        soumetteur="OVMF du banc",
        droit_restituer=False,
        droit_ecrire=False,
        motif=reason,
    )
