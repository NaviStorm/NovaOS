"""Attribution externe des EOI sur le seul firmware OVMF identifié dans le banc."""

import hashlib
import json
from pathlib import Path
import struct

from verdict_apic import juger
from banc import Debug
from sonde import hmp

TRACE = (
    "memory_region_ops_write",
    "memory_region_subpage_write",
    "apic_local_deliver",
    "apic_deliver_irq",
    "pci_cfg_write",
    "vtd_reg_write",
    "pci_nvme_irq_msix",
    "pci_nvme_irq_pin",
    "virtio_notify",
)
TIMER = "3190d6c376507df605ffb7dd99f5af4773a0ad62e26f247d4a2f67c62926b854"
EFI = "96a5a634b9a90f485b6bdfa0ac74f80a2d56d8819ff386d96c213b771f121239"


def check(ok, cause):
    if not ok:
        raise ValueError(cause)


def epingle(path, sha):
    data = path.read_bytes()
    check(hashlib.sha256(data).hexdigest() == sha, "PE inattendu")
    return data


def mem(d, adr, n):
    data = bytes.fromhex(d.command(f"m{adr:x},{n:x}"))
    check(len(data) == n, "Lecture GDB incomplète")
    return data


def observer(q, j, cal, *, diagnostic=None, verifier_entree=None, evenements=TRACE):
    (timer,) = cal["timers"]
    entry = cal["entree"]
    term = cal["image"] + (diagnostic["hlt"] if diagnostic else 0x3578)
    handler, store = timer + 0x4B9, timer + 0x97F
    module = epingle(
        j / "ovmf5j-b/Build/OvmfX64/RELEASE_CLANGDWARF/X64/LocalApicTimerDxe.efi", TIMER
    )
    image = epingle(diagnostic["fichier"], diagnostic["sha256"]) if diagnostic else epingle(j / "supports5j-b/diagnostic.efi", EFI)
    (cpu,) = q.call("query-cpus-fast")
    check(cpu["cpu-index"] == 0, "vCPU inattendu")
    dbg = Debug()
    dbg.watch(entry)
    dbg.resume()
    check(dbg.pc() == entry, "Entrée inattendue")
    dbg.remove(entry)
    raw = diagnostic["offset_entree"] if diagnostic else entry - cal["image"] - 0xC00
    check(
        mem(dbg, entry, 16) == image[raw : raw + 16]
        and mem(dbg, term, 3) == b"\xf4\xeb\xfd",
        "Diagnostic non identifié",
    )
    for a, b in ((0x4B9, 0x544), (0x962, 0x985)):
        check(mem(dbg, timer + a, b - a) == module[a:b], "Gestionnaire non identifié")
    for ev in evenements:
        q.call("trace-event-set-state", name=ev, enable=True)
    trace = Path("trace.log")
    start = trace.stat().st_size
    if verifier_entree:
        verifier_entree(q, dbg)
    dbg.watch(term)
    dbg.watch(handler)
    terminal = False
    pending = None
    rows = []
    for _ in range(512):
        dbg.resume()
        regs = struct.unpack_from("<17Q", bytes.fromhex(dbg.command("g")))
        pc = regs[16]
        offset = trace.stat().st_size - start
        if pc == term:
            dbg.remove(term)
            if terminal:
                break
            terminal = True
        elif pc == handler:
            check(pending is None, "Gestionnaire imbriqué")
            pending = dict(
                entree=offset,
                vecteur=regs[2],
                lapic=hmp(q, "info lapic"),
                vcpu=cpu["cpu-index"],
                gestionnaire=pc,
                apres_hlt=terminal,
            )
            dbg.remove(handler)
            dbg.watch(store)
        elif pc == store:
            check(pending is not None, "EOI sans gestionnaire")
            pending.update(
                avant_ecriture=offset,
                instruction=pc,
                adresse=regs[0] + regs[14],
                valeur=regs[1] & 0xFFFFFFFF,
                retour=int.from_bytes(mem(dbg, regs[7] + 0x58, 8), "little"),
            )
            rows.append(pending)
            pending = None
            dbg.remove(store)
            dbg.watch(handler)
            if terminal:
                dbg.watch(term)
        else:
            raise ValueError("Point d’arrêt inattendu")
    else:
        raise ValueError("Intervalle APIC incomplet")
    check(pending is None and terminal, "Fin d'intervalle absente")
    dbg.remove(handler)
    end = trace.stat().st_size
    for ev in evenements:
        q.call("trace-event-set-state", name=ev, enable=False)
    q.observateur = dbg
    buf = trace.read_bytes()[start:end]
    Path("intervalle.log").write_bytes(buf)
    proof = dict(
        timer=timer,
        entree=entry,
        hlt=term,
        lignes=rows,
        debut=start,
        fin=end,
        sha_module=TIMER,
        sha_diagnostic=diagnostic["sha256"] if diagnostic else EFI,
    )
    Path("interruptions.json").write_text(json.dumps(proof, indent=2) + "\n")
    verdict = juger(trace.read_bytes(), buf, proof)
    Path("verdict-apic.json").write_text(json.dumps(verdict, indent=2) + "\n")
