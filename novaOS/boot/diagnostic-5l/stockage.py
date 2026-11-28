"""Oracle du retrait NIC quand le bail NVMe est encore en service."""

import json
import re
import struct


def exiger(ok, msg):
    if not ok:
        raise ValueError(msg)


def domaine_final(vm, qt, serie, out, iqe):
    from retrait_duo import lire_page
    from virtio_net_confinement import reg

    rows = re.findall(
        rb"DUO inventaire controleur=(\[[0-9, ]+\]) disque=(\[[0-9, ]+\]) reseau=(\[[0-9, ]+\])",
        serie,
    )
    ctl, disk, nic = map(json.loads, rows[0]) if len(rows) == 1 else ([], [], [])
    cadre = ctl + disk + nic
    exiger(
        [len(ctl), len(disk), len(nic)] == [8, 6, 26]
        and len(set(cadre)) == 40
        and all(0 < x < 256 << 20 and not x % 4096 for x in cadre),
        "Inventaire de pages",
    )
    brut = b"".join(struct.pack("<512Q", *lire_page(qt, p)) for p in ctl)
    (out / "tables-qi.bin").write_bytes(brut)
    lo = lambda o: reg(vm, 0xFED90000 + o)
    u64 = lambda o: lo(o) | (lo(o + 4) << 32)
    regs = dict(
        root=u64(0x20),
        qi=u64(0x90),
        iqh=u64(0x80),
        iqt=u64(0x88),
        gsts=lo(0x1C),
        fsts=lo(0x34),
        wait=reg(vm, ctl[7]),
    )
    exiger(
        regs
        == dict(
            root=ctl[0],
            qi=ctl[6],
            iqh=160 if iqe else 176,
            iqt=192,
            gsts=0xC4000000,
            fsts=16 if iqe else 0,
            wait=0,
        ),
        "File commune ou racine",
    )
    return dict(registres=regs, controleur=ctl, disque=disk, nic=nic)


def verifier_actif(out, lecteur, avant, pair, panne):
    from nvme_phase2_oracle import inspect, io_trace, verify_transition

    trace = (out / "qemu.trace").read_text()
    serie = (out / "serie.log").read_text()
    apres = inspect(lecteur, out / "namespace.raw", 512)
    verify_transition(avant, apres, {"ancien"})
    marque = (
        "NOVA WAIT hold mode=absent cookie=4 head=11"
        if panne == "wait"
        else "invalid inv desc"
    )
    exiger(trace.count(marque) == 1 and not pair.error, "Panne ou pair non corroborés")
    debut, fin = trace.split(marque)
    ops = io_trace(out / "qemu.trace", 512)
    cmds = re.findall(r"pci_cfg_write nvme 00:06\.0 @0x4 <- (0x\w+)", debut)
    exiger(
        int(cmds[-1], 16) & 4 and "pci_cfg_write nvme 00:06.0 @0x4 <- 0x402" in fin,
        "BME NVMe pendant la panne",
    )
    exiger(
        len(ops) == 1
        and ops[0]["op"] == "READ"
        and ops[0]["cqe"] == 0
        and "pci_nvme_io_cmd " not in fin
        and "NVME bilan ok=false" in serie,
        "Arrêt du stockage",
    )
    exiger(
        "panne NIC non isolée" in serie
        and "E3D collecte NIC" not in serie
        and "liberes=26" not in serie
        and "NVME ressources initial=0 final=40" in serie
        and "NVME P9 statut bloc=0b" in serie,
        "Décision du superviseur",
    )
    return dict(p9=apres, operations_avant=len(ops), operations_apres=0)
