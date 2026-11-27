"""Lectures typées du modèle arrêté. Aucun point d'entrée ne prend une adresse de l'invité."""

import re

# Intel VT-d, registres d'identification, d'état et pointeur de racine.
VTD = {
    "ver": (0, 4),
    "cap": (8, 8),
    "ecap": (16, 8),
    "gsts": (28, 4),
    "rtaddr": (32, 8),
}
PCI = {
    "ident": (0, 4),
    "cmd": (4, 2),
    "classe_revision": (8, 4),
    "header": (14, 1),
    "bus_pont": (24, 4),
}


def hmp(q, cmd):
    return q.call("human-monitor-command", **{"command-line": cmd})


def windows(tree, model):
    matches = set()
    for line in tree.splitlines():
        if line.strip().endswith(": " + model):
            m = re.fullmatch(
                r"\s*([\da-f]+)-([\da-f]+) \(prio \d+, i/o\): " + model, line
            )
            if not m:
                raise ValueError("Fenêtre non reconnue " + line)
            matches.add(tuple(int(x, 16) for x in m.groups()))
    return sorted(matches)


def _register(q, window, offset, width):
    base, last = window
    addr = base + offset
    if not (base <= addr <= last - width + 1) or addr % width:
        raise ValueError("Accès hors de la fenêtre attestée par le modèle")
    # QEMU xp /g peut fusionner deux accès MMIO 32 bits dans le dispositif.
    fmt = {1: "b", 2: "h", 4: "w", 8: "g"}[width]
    line = hmp(q, f"xp /1{fmt}x {addr:#x}").strip()
    m = re.fullmatch(r"([0-9a-fA-F]+):\s+0x([0-9a-fA-F]+)", line)
    if not m or int(m[1], 16) != addr or len(m[2]) != width * 2:
        raise ValueError("Relecture ambiguë " + line)
    return int(m[2], 16)


def devices(buses):
    def walk(rows):
        for dev in rows:
            yield dev
            yield from walk(dev.get("pci_bridge", {}).get("devices", []))

    for root in buses:
        yield from walk(root["devices"])


def cpu_flatview(tree):
    views = re.split(r"(?m)^FlatView #\d+\s*\n", tree)
    cpu = [
        v
        for v in views
        if re.search(r'AS "cpu-memory-[^"]+"|AS "memory", root: system', v)
    ]
    if len(cpu) != 1:
        raise ValueError("FlatView CPU absent ou ambigu")
    return cpu[0]


def state(q):
    if q.call("query-status")["running"]:
        raise ValueError("La sonde exige une machine suspendue")
    tree = cpu_flatview(hmp(q, "info mtree -f"))
    units = {}
    remappers = windows(tree, "intel_iommu")
    if len(remappers) > 16:
        raise ValueError("Trop d’unités VT-d")
    for bounds in remappers:
        unit = {k: _register(q, bounds, *reg) for k, reg in VTD.items()}
        units[hex(bounds[0])] = unit
    apertures = windows(tree, "pcie-mmcfg-mmio")
    pci = {}
    # QMP query-pci ne fournit pas le segment. L'adaptateur Q35 ne joint que
    # sa fenêtre unique de bus zéro. Toute autre géométrie requiert un autre adaptateur.
    if len(apertures) > 1:
        raise ValueError("Association ECAM/segment non établie")
    if len(apertures) == 1:
        for dev in devices(q.call("query-pci")):
            if len(pci) >= 64:
                raise ValueError("Trop de fonctions PCI")
            bus, slot, func = (dev[k] for k in ("bus", "slot", "function"))
            if not (0 <= bus < 256 and 0 <= slot < 32 and 0 <= func < 8):
                raise ValueError("RID hors bornes")
            key = f"0000:{bus:02x}:{slot:02x}.{func}"
            if key in pci:
                raise ValueError("RID dupliqué")
            start = bus << 20 | slot << 15 | func << 12

            def cfg(name):
                off, size = PCI[name]
                return _register(q, apertures[0], start + off, size)

            row = {k: cfg(k) for k in ("ident", "cmd", "classe_revision", "header")}
            if row["header"] & 127 == 1:
                row["bus_pont"] = cfg("bus_pont")
            pci[key] = row
    return {"unites": units, "pci": pci, "ecam": apertures}
