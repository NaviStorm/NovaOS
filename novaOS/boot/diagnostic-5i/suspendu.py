"""Chemin passif qualifiable : arrêter au premier RIP EFI et terminer la VM encore suspendue."""

import argparse
import json
from pathlib import Path
import sys
import time
import struct
import subprocess
from acpi_fige import tables
from sonde import state

NOVA = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(NOVA / "boot/diagnostic-5e"), str(NOVA / "tests/ethernet_v1")]
from banc import guest, digest
from port_qemu import qualification


def decode(blobs, model):
    m = blobs["MCFG"]
    if len(m) < 60 or (len(m) - 44) % 16:
        raise ValueError("MCFG")
    w = []
    for base, seg, b, e in struct.iter_unpack("<QHBB4x", m[44:]):
        if base & 0xFFFFF or e < b:
            raise ValueError("ECAM invalide")
        w.append((base, seg, b, e))
    if (
        len(w) != 1
        or w[0][1:3] != (0, 0)
        or model["ecam"] != [(w[0][0], w[0][0] + ((w[0][3] + 1) << 20) - 1)]
    ):
        raise ValueError("Géométrie hors du témoin Q35 de la sonde")
    units = []
    data = blobs.get("DMAR", b"")
    if data and len(data) < 48:
        raise ValueError("DMAR tronquée")
    off = 48
    while off < len(data):
        if off + 4 > len(data):
            raise ValueError("Structure DMAR tronquée")
        kind, n = struct.unpack_from("<HH", data, off)
        if n < 4 or off + n > len(data):
            raise ValueError("Longueur DMAR")
        if kind == 0:
            if n < 16:
                raise ValueError("DRHD tronquée")
            base = struct.unpack_from("<Q", data, off + 8)[0]
            if not base or base % 4096 or base in units:
                raise ValueError("Base DRHD")
            units.append(base)
        off += n
    return {
        "ecam": w,
        "drhd_decrites": units,
        "drhd_corroborees": [a for a in units if hex(a) in model["unites"]],
        "propriete_dma": "indeterminee",
        "activation": False,
    }


def observe(q, phase, reader):
    start = time.perf_counter_ns()
    model = state(q)
    blobs = tables(q)
    result = decode(blobs, model)
    elapsed = time.perf_counter_ns() - start
    if state(q) != model or blobs != tables(q):
        raise ValueError("Instabilité de la photographie")
    for key, value in blobs.items():
        Path(key + ".bin").write_bytes(value)
    rows = []
    for rid, pci in model["pci"].items():
        seg, bus, slot = rid.split(":")
        dev, fun = slot.split(".")
        rows.append(
            [int(x, 16) for x in (seg, bus, dev, fun)]
            + [
                pci["ident"],
                pci["cmd"],
                pci["classe_revision"] >> 8,
                pci["header"],
                pci.get("bus_pont", 0),
            ]
        )
    Path("fonctions.txt").write_text(
        "".join(" ".join(map(str, row)) + "\n" for row in rows)
    )
    report = subprocess.check_output([reader, "."], text=True)
    Path("portees.txt").write_text(report)
    result["portees"] = {
        r: {"drhd": base, "rmrr": n, "groupe_possible": g, "taille": int(k)}
        for r, base, n, g, k in (line.split() for line in report.splitlines())
    }
    Path("inventaire.json").write_text(
        json.dumps(dict(result, modele=model, duree_ns=elapsed), indent=2) + "\n"
    )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for k in ("supports", "firmware", "calibrations", "sortie", "worktrees", "lecteur"):
        p.add_argument(k, type=Path)
    a = p.parse_args()
    supports, fw, cal, out, wt = [
        p.resolve()
        for p in (a.supports, a.firmware, a.calibrations, a.sortie, a.worktrees)
    ]
    out.mkdir(parents=True, exist_ok=False)
    hist = json.loads((cal / "bilan.json").read_text())
    result = {"garde": {}, "cas": {}, "firmware": digest(fw)}
    if len(hist["cas"]) != 11:
        raise ValueError("Matrice de calibration incomplète")
    if result["firmware"] != hist["firmware"]:
        raise ValueError("Firmware différent")
    from point_entree import entry_witness

    with qualification(wt, result["garde"]):
        for name, row in hist["cas"].items():
            media = supports / (row["support"] + ".img")
            store = supports / (row["mode"] + ".fd")
            table = cal / (name + ".bin")
            if digest(media) != row["image"] or digest(store) != row["magasin"]:
                raise ValueError("Image différente de la calibration")
            # Une table invalide doit conduire à un refus contrôlé. Le décodeur de
            # structure n'a pas à lire un registre à l'adresse indiquée dans la DMAR.
            reader = a.lecteur.resolve()

            def sample(q, phase):
                witness = entry_witness(q, media, row["rapport"]["entree"])
                Path("point-entree.json").write_text(
                    json.dumps(witness, indent=2) + "\n"
                )
                if name == "invalide":
                    try:
                        observe(q, phase, reader)
                    except ValueError as e:
                        if str(e) != "Longueur DMAR":
                            raise
                        Path("refus.txt").write_text(str(e) + "\n")
                    else:
                        raise ValueError("DMAR invalide acceptée")
                else:
                    observe(q, phase, reader)

            run = guest(
                out / name,
                media,
                store,
                fw,
                row["topologie"],
                expected=row["rapport"],
                table=table if table.is_file() else None,
                observe=sample,
                stop_at_entry=True,
                interval_events=(
                    "memory_region_ops_write",
                    "memory_region_subpage_write",
                ),
            )
            if not run["disque_intact"] or any(run["apres"].values()):
                raise ValueError("Stockage")
            if name != "invalide":
                data = json.loads((out / name / "inventaire.json").read_text())
                prior = json.loads((cal / name / "inventaire.json").read_text())
                if (
                    data["portees"] != prior["portees"]
                    or data["modele"] != prior["modele"]
                ):
                    raise ValueError(
                        "Deux démarrages indépendants ont des états différents"
                    )
            result["cas"][name] = run
    (out / "bilan.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
