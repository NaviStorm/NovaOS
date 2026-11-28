"""Contrôle indépendant des points d'arrêt GDB et des événements du modèle QEMU."""

import re

EOI = re.compile(
    rb"memory_region_ops_write cpu 0 mr 0x[0-9a-f]+ addr 0xfee000b0 value 0x0 size 4 name 'apic-msi'"
)
REG = b"apic_register_write register 0x0b = 0x0"
SOURCE = b"apic_local_deliver vector 0 delivery mode 0"
INT = b"Servicing hardware INT=0x20"
CANDIDAT = (
    b"memory_region_",
    b"apic_register_write",
    b"vtd_reg_write",
    b"pci_cfg_write",
)
AUTRE_IRQ = (b"apic_deliver_irq", b"pci_nvme_irq", b"virtio_notify")


def juger(trace, intervalle, preuve):
    debut, fin = preuve["debut"], preuve["fin"]
    if not (0 <= debut < fin <= len(trace)) or trace[debut:fin] != intervalle:
        raise ValueError("Intervalle incomplet")
    rows = preuve["lignes"]
    if not rows or not rows[-1]["apres_hlt"]:
        raise ValueError("EOI terminal absent")
    timer = preuve["timer"]
    allowed = set()
    last = None
    derniere_livraison = -1
    for row in rows:
        a, b = row["entree"], row["avant_ecriture"]
        if not (0 <= a <= b < len(intervalle)) or (last is not None and a <= last):
            raise ValueError("Chronologie incohérente")
        last = b
        lines = intervalle[b:].splitlines(keepends=True)
        if len(lines) < 2:
            raise ValueError("Trace EOI tronquée")
        # Un arrêt d'exécution précède immédiatement l'instruction de stockage.
        if not (EOI.fullmatch(lines[0].rstrip()) and lines[1].rstrip() == REG):
            raise ValueError("Écriture xAPIC non attribuée")
        prefix = trace[: debut + a]
        livraison = prefix.rfind(INT + b"\n")
        ancienne = prefix.rfind(INT + b"\n", 0, livraison)
        if livraison <= derniere_livraison:
            raise ValueError("Vecteur absent")
        derniere_livraison = livraison
        origine = prefix[max(0, ancienne) : livraison].splitlines()
        livraison_concurrente = any(
            s.startswith(AUTRE_IRQ)
            or (s.startswith(b"apic_local_deliver") and s != SOURCE)
            for s in origine
        )
        if (
            SOURCE not in origine
            or livraison_concurrente
            or any(
                s.startswith(b"Servicing hardware INT=") and s != INT
                for s in prefix[livraison:].splitlines()
            )
        ):
            raise ValueError("Source de l'interruption ambiguë")
        lapic = row["lapic"].splitlines()
        if not (
            row["vcpu"] == 0
            and row["vecteur"] == 32
            and row["gestionnaire"] == timer + 0x4B9
            and row["instruction"] == timer + 0x97F
            and row["retour"] == timer + 0x4DF
            and row["adresse"] == 0xFEE000B0
            and row["valeur"] == 0
            and any(re.fullmatch(r"ISR\s+32\s*", s) for s in lapic)
            and any(
                re.fullmatch(r"LVTT\s+0x00020020\s+.*periodic\s+Fixed\s+\(vec 32\)", s)
                for s in lapic
            )
            and any(s.strip() == "dumping local APIC state for CPU 0" for s in lapic)
        ):
            raise ValueError("Gestionnaire, appelant, vCPU ou LAPIC incohérent")
        allowed.update((b, b + len(lines[0])))
    refus = []
    pos = 0
    for line in intervalle.splitlines(keepends=True):
        if line.startswith(CANDIDAT) and pos not in allowed:
            refus.append(dict(offset=pos, ligne=line.decode().strip()))
        elif line.startswith(AUTRE_IRQ):
            raise ValueError("IRQ concurrente")
        pos += len(line)
    return dict(
        eoi_attribues=len(rows), ecritures_refusees=refus, intervalle_admis=not refus
    )
