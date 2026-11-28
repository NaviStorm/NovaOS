"""Lectures du modèle Q35 arrêté et attribution des structures xHCI dans la RAM du banc."""

from pathlib import Path
import re
import struct
import sys

NOVA = Path(__file__).resolve().parents[2]
for phase in ("i", "o"):
    sys.path.insert(0, str(NOVA / "boot" / ("diagnostic-5" + phase)))
from image5o import require
from demarrage import ram
from sonde import state, devices, _register, hmp, cpu_flatview


def etendue(addr, length, align, spans):
    if not (
        isinstance(addr, int)
        and addr > 0
        and addr % align == 0
        and 0 < length <= 65536
        and sum(lo <= addr and addr + length <= hi for lo, hi in spans) == 1
    ):
        raise ValueError("Adresse hors de la RAM ordinaire attestée")
    return addr


def plages(q):
    # Uniquement pc.ram dans la vue CPU, y compris les trous MMIO réels du Q35.
    view = cpu_flatview(hmp(q, "info mtree -f"))
    return [
        (int(a, 16), int(b, 16) + 1)
        for a, b in re.findall(
            r"(?m)^\s*([0-9a-f]+)-([0-9a-f]+) \(prio -?\d+, ram\): pc\.ram(?:\s|$)",
            view,
        )
    ]


def instantane(q):
    board = state(q)
    spans = plages(q)
    units = board["unites"]
    identite_dma = len(units) == 1 and all(
        not (u["gsts"] & (1 << 31)) for u in units.values()
    )
    controllers = {}
    for d in devices(q.call("query-pci")):
        bus, slot, fn = (d[k] for k in ("bus", "slot", "function"))
        key = f"0000:{bus:02x}:{slot:02x}.{fn}"
        pci = board["pci"][key]
        if pci["classe_revision"] >> 8 != 0x0C0330:
            continue
        (bar,) = [r for r in d["regions"] if r["bar"] == 0 and r["type"] == "memory"]
        out = controllers[key] = dict(
            qdev=d["qdev_id"], bme=bool(pci["cmd"] & 4), bar=bar["address"], slots=[]
        )
        if bar["address"] < 0 or not pci["cmd"] & 2:
            continue
        base = bar["address"]
        win = (base, base + bar["size"] - 1)

        def word(off):
            return _register(q, win, off, 4)

        def pair(off):
            return word(off) | (word(off + 4) << 32)

        cap = word(0)
        op = cap & 255
        hcs = word(4)
        ports = hcs >> 24
        runtime = word(0x18) & ~31
        count = word(op + 0x38) & 255
        width = 64 if word(0x10) & 4 else 32
        require(
            0x20 <= op <= 0x100
            and 0 < count <= hcs & 255
            and count <= 128
            and ports <= 64,
            "Capacités xHCI",
        )
        dcbaa = pair(op + 0x30) & ~63
        cmd = pair(op + 0x18) & ~63
        segcount = word(runtime + 0x28) & 0xFFFF
        erst = pair(runtime + 0x30) & ~63
        out.update(
            version=cap >> 16,
            commande=cmd,
            dcbaa=dcbaa,
            erst=erst,
            segments=segcount,
            cycle_commande=word(op + 0x18) & 1,
            run=bool(word(op) & 1),
            context_size=width,
            dequeue_evenement=pair(runtime + 0x38) & ~15,
            ports=[i + 1 for i in range(ports) if word(op + 0x400 + i * 16) & 1],
        )
        protocoles = []
        offset = (word(0x10) >> 16) * 4
        for _ in range(32):
            if not offset:
                break
            require(
                0x20 <= offset < op and offset + 12 <= op, "Capacité xHCI hors fenêtre"
            )
            header = word(offset)
            if header & 255 == 2:
                require(word(offset + 4) == 0x20425355, "Protocole de port USB inconnu")
                val = word(offset + 8)
                first, total = val & 255, (val >> 8) & 255
                require(
                    first > 0 and 0 < total <= ports - first + 1,
                    "Plage de ports invalide",
                )
                protocoles.append([header >> 24, first, total])
            step = (header >> 8) & 255
            offset = offset + step * 4 if step else 0
        require(not offset, "Trop de capacités xHCI")
        out["protocoles"] = protocoles
        if not identite_dma or not dcbaa or not spans:
            continue

        def lire(addr, n, align):
            return ram(q, etendue(addr, n, align, spans), n)

        def qword(data, pos):
            return struct.unpack_from("<Q", data, pos)[0]

        require(0 < segcount <= 16 and cmd, "Anneaux du contrôleur non définis")
        etendue(cmd, 16, 64, spans)
        entries = lire(erst, segcount * 16, 64)
        out["evenements"] = []
        for i in range(segcount):
            buf, n, _ = struct.unpack_from("<QII", entries, 16 * i)
            require(16 <= n <= 4096, "Segment d’événements hors profil")
            etendue(buf, n * 16, 64, spans)
            out["evenements"].append([buf, n])
        contexts = lire(dcbaa, (1 + count) * 8, 64)
        for i in range(1, count + 1):
            address = qword(contexts, i * 8)
            if not address:
                continue
            context = lire(address, 32 * width, 64)
            a, b, _, state_slot = struct.unpack_from("<4I", context)
            if not state_slot >> 27:
                continue
            eps = []
            for ep in range(1, 1 + (a >> 27)):
                w0, w1, deq = struct.unpack_from("<IIQ", context, ep * width)
                if not w0 & 7:
                    continue
                if (w0 >> 10) & 31:
                    raise ValueError("Anneau de streams hors profil")
                etendue(deq & ~15, 16, 16, spans)
                eps.append(
                    dict(
                        dci=ep,
                        type=(w1 >> 3) & 7,
                        etat=w0 & 7,
                        dequeue=deq & ~15,
                        cycle=deq & 1,
                    )
                )
            out["slots"].append(
                dict(
                    slot=i,
                    adresse_usb=state_slot & 255,
                    route=a & 0xFFFFF,
                    port_racine=b >> 16 & 255,
                    contexte=address,
                    etat=state_slot >> 27,
                    endpoints=eps,
                )
            )
    return dict(
        controleurs=controllers,
        vtd=units,
        ram=spans,
        bus_usb=hmp(q, "info usb").strip().splitlines(),
    )


def activite(lines):
    """Apparier des transferts du modèle par identité de requête, jamais par silence de la file."""
    vols = {}
    anomalies = 0
    termines = 0
    asyncs = 0
    maxvols = 0
    luns = set()
    adresses = []
    tampons = 0
    for line in lines:
        m = re.fullmatch(
            r"usb_xhci_xfer_start (0x[0-9a-f]+): slotid (\d+), epid (\d+), streamid (\d+)",
            line,
        )
        if m:
            if m[1] in vols:
                anomalies += 1
            vols[m[1]] = (int(m[2]), int(m[3]), False)
        m = re.fullmatch(r"usb_xhci_xfer_async (0x[0-9a-f]+)", line)
        if m and m[1] not in vols:
            anomalies += 1
        if m and m[1] in vols:
            s, e, _ = vols[m[1]]
            vols[m[1]] = (s, e, True)
            asyncs += 1
        m = re.match(r"usb_xhci_xfer_(?:success|error) (0x[0-9a-f]+):", line)
        if m:
            if vols.pop(m[1], None) is not None:
                termines += 1
            else:
                anomalies += 1
        m = re.match(r"usb_msd_cmd_submit lun (\d+),", line)
        if m:
            luns.add(int(m[1]))
        m = re.fullmatch(r"usb_xhci_slot_address slotid (\d+), port ([\d.]+)", line)
        if m:
            adresses.append([int(m[1]), m[2]])
        if line.startswith("usb_xhci_fetch_trb ") and ", TR_NORMAL," in line:
            tampons += 1
        maxvols = max(maxvols, len(vols))
    return dict(
        anomalies=anomalies,
        asyncs=asyncs,
        termines=termines,
        maximum=maxvols,
        vols=sorted(v for v in vols.values()),
        luns=sorted(luns),
        adresses=adresses,
        trb_normaux=tampons,
    )
