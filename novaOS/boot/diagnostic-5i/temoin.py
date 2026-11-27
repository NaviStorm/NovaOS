"""Qualification passive 5I. Entrées exclusivement synthétiques, répertoire de sortie neuf."""

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
from collections import Counter
import time

from sonde import state

NOVA = Path(__file__).resolve().parents[2]
sys.path[:0] = [
    str(NOVA / "boot/diagnostic-5f"),
    str(NOVA / "boot/diagnostic-5e"),
    str(NOVA / "tests/ethernet_v1"),
]
from banc import guest, digest
from observateur import sauver
from port_qemu import qualification


def require(p, msg):
    if not p:
        raise ValueError(msg)


def observe(q, phase):
    trace = Path("transactions.log")
    checkpoint = trace.stat().st_size
    before = q.stats()
    start = time.perf_counter_ns()
    first = state(q)
    elapsed = time.perf_counter_ns() - start
    require(
        first == state(q)
        and before == q.stats()
        and trace.stat().st_size == checkpoint,
        "Mutation pendant la sonde suspendue " + phase,
    )
    sauver(Path("modele-" + phase + ".json"), first)
    sauver(
        Path("sonde-" + phase + ".json"),
        {
            "duree_ns": elapsed,
            "trace_octets": checkpoint,
            "deux_lectures_identiques": True,
            "io": before,
        },
    )


def dmar_file(dest, records):
    import struct

    data = bytearray(
        struct.pack(
            "<4sIBB6s8sI4sI",
            b"DMAR",
            48 + len(records),
            1,
            0,
            b"NOVALB",
            b"PASSIF5I",
            1,
            b"NOVA",
            1,
        )
        + bytes([47])
        + bytes(11)
        + records
    )
    data[9] = -sum(data) & 255
    dest.write_bytes(data)


def qualify(out, name, topo, store, lab, table=None, status=16):
    kw = dict(
        media=lab["supports"] / "diagnostic.img",
        variables=lab["supports"] / (store + ".fd"),
        firmware=lab["fw"],
        topology=topo,
        table=table,
    )
    cal = guest(out / (name + "-cal"), **kw)
    require(
        cal["rapport"] and cal["rapport"]["statut"] == status, "Calibration " + name
    )
    dest = out / name
    run = guest(
        dest,
        expected=cal["rapport"],
        observe=observe,
        observe_reset=True,
        interval_events=("memory_region_ops_write", "memory_region_subpage_write"),
        **kw,
    )
    report = run["rapport"]
    require(
        report
        and report["statut"] == status
        and run["halt"]
        and not report["noeuds"]
        and report["vivants"] == 0,
        "Arrêt ou mémoire " + name,
    )
    require(
        run["disque_intact"]
        and run["entree"] == run["fin"]
        and not any(run["fin"].values()),
        "E/S internes " + name,
    )
    # Le verdict global reste négatif pour TOUT événement, sans exception.
    # Continuer le recueil permet d'archiver tous les échecs et les contrôles indépendants.
    events = run["evenements_diagnostic"]
    strict = not events and run["octets_avant"] == run["octets_avant_sortie"]
    states = {
        p: json.loads((dest / ("modele-" + p + ".json")).read_text())
        for p in ("reset", "entree", "sortie", "fin")
    }
    require(
        states["entree"] == states["sortie"] == states["fin"], "Registres divergents"
    )
    log = (dest / "journal-5i.txt").read_text(errors="replace")
    require(
        ("NovaOS: Secure Boot " + ("actif -" if store == "nominal" else "inactif -"))
        in log,
        "Mode EFI",
    )
    guest_pci = {}
    for rid, ident, cls, cmd, hdr, unit, rmrr in re.findall(
        r"5I RID (\S+) id=(\w+) cls=(\w+) cmd=(\w+) hdr=(\w+) drhd=(\w+) rmrr=(\w+)",
        log,
    ):
        require(rid not in guest_pci, "RID EFI double")
        row = states["entree"]["pci"][rid]
        require(
            [int(s, 16) for s in (ident, cls, cmd, hdr)]
            == [row["ident"], row["classe_revision"] >> 8, row["cmd"], row["header"]],
            "PCI différent " + rid,
        )
        guest_pci[rid] = {"unite_candidate": unit, "rmrr": rmrr}
    ticks = re.findall(r"5I scan_tsc=(\d+) functions=(\d+)", log)
    if status == 16:
        require(
            len(ticks) == 1 and len(guest_pci) == int(ticks[0][1]) > 0,
            "Inventaire incomplet",
        )
        require(
            any(
                states["entree"]["pci"][k]["classe_revision"] >> 24 == 2
                for k in guest_pci
            )
            and any(
                states["entree"]["pci"][k]["classe_revision"] >> 24 == 1
                for k in guest_pci
            ),
            "NIC/stockage absents",
        )
    else:
        require(not ticks and not guest_pci, "Publication sur refus ACPI")
    groups = dict(
        (rid, (head, int(n)))
        for rid, head, n in re.findall(
            r"5I possible-group (\S+) ref=(\S+) count=(\d+)", log
        )
    )
    if ticks:
        require(
            set(guest_pci) == set(states["entree"]["pci"]) == set(groups),
            "Couverture PCI",
        )
        for rid, (head, n) in groups.items():
            require(
                head in groups and sum(v[0] == head for v in groups.values()) == n,
                "Groupe incohérent",
            )
        sata = next(
            k
            for k in guest_pci
            if states["entree"]["pci"][k]["classe_revision"] >> 16 == 0x0106
        )
        nic = next(
            k
            for k in guest_pci
            if states["entree"]["pci"][k]["classe_revision"] >> 16 == 0x0200
        )
        if name == "ambigu":
            require(
                guest_pci[sata]["unite_candidate"] == "unknown",
                "Double attribution acceptée",
            )
        elif name == "deux":
            require(
                guest_pci[sata] == {"unite_candidate": "30000000", "rmrr": "1"}
                and guest_pci[nic] == {"unite_candidate": "30001000", "rmrr": "0"}
                and not states["entree"]["unites"],
                "Confusion entre DRHD déclaré et réel",
            )
        elif topo == "pont":
            require(
                len(states["entree"]["unites"]) == 1
                and next(iter(states["entree"]["unites"].values()))["gsts"] & (1 << 31)
                == 0
                and int(guest_pci[sata]["unite_candidate"], 16)
                == int(next(iter(states["entree"]["unites"])), 16)
                and guest_pci[nic] == guest_pci[sata]
                and groups[nic][1] >= 2,
                "Unité commune ou pont",
            )
    return {
        "topologie": topo,
        "magasin": store,
        "statut": status,
        "rapport": report,
        "modele": states["entree"],
        "pci_efi": guest_pci,
        "cycles_collecte": int(ticks[0][0]) if ticks else None,
        "io_entree": run["entree"],
        "io_fin": run["fin"],
        "disque_intact": run["disque_intact"],
        "image": run["image"],
        "zero_ecriture_efi": strict,
        "evenements": dict(
            Counter(
                line.rsplit("name ", 1)[-1] if "name " in line else line.split()[0]
                for line in events
            )
        ),
        "coupe_entree": run["octets_avant"],
        "coupe_sortie": run["octets_avant_sortie"],
        "sha_disque_final": digest(dest / "interne.raw"),
    }


def fixtures(out):
    import struct

    unit = lambda addr, all_, sc: (
        struct.pack("<HHBBHQ", 0, 16 + len(sc), all_, 0, 0, addr) + sc
    )
    endpoint = bytes([1, 8, 0, 0, 0, 0, 31, 2])
    # Descriptions injectées sans MMIO correspondant. Leur adresse ne donne aucun droit.
    first = unit(0x30000000, 0, endpoint)
    rmrr = struct.pack("<HHHHQQ", 1, 32, 0, 0, 0x26000000, 0x26000FFF) + endpoint
    dmar_file(out / "deux.bin", first + unit(0x30001000, 1, b"") + rmrr)
    dmar_file(out / "ambigu.bin", first + unit(0x30001000, 0, endpoint))
    dmar_file(out / "invalide.bin", b"\x00\x00\x03\x00")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for k in ("supports", "firmware", "sortie", "worktrees"):
        p.add_argument(k, type=Path)
    a = p.parse_args()
    lab = {"supports": a.supports.resolve(), "fw": a.firmware.resolve()}
    out = a.sortie.resolve()
    out.mkdir(parents=True, exist_ok=False)
    result = {
        "base": "4489fce73d8fe66b3406679dbb6d564d05a6c54d",
        "garde": {},
        "cas": {},
        "qemu": subprocess.check_output(
            ["qemu-system-x86_64", "--version"], text=True
        ).splitlines()[0],
        "firmware": digest(lab["fw"]),
    }
    fixtures(out)
    with qualification(a.worktrees.resolve(), result["garde"]):
        for topo in ("direct", "pont"):
            for store in ("nominal", "inactif"):
                name = topo + "-" + store
                result["cas"][name] = qualify(out, name, topo, store, lab)
                sauver(out / "bilan.json", result)
        for name, stat in (("deux", 16), ("ambigu", 16), ("invalide", 5)):
            result["cas"][name] = qualify(
                out, name, "direct", "nominal", lab, out / (name + ".bin"), stat
            )
            sauver(out / "bilan.json", result)
    result["efi_qualifie"] = all(
        case["zero_ecriture_efi"] for case in result["cas"].values()
    )
    sauver(out / "bilan.json", result)
    if not result["efi_qualifie"]:
        raise SystemExit(
            "REFUS 5I : au moins un intervalle EFI comporte des écritures ; voir les traces."
        )


if __name__ == "__main__":
    main()
