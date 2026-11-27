"""Campagne 5E sur images régulières fictives, sous le verrou commun NovaOS."""

import argparse
import json
import re
from pathlib import Path
import struct
import subprocess
import sys

from banc import digest, guest

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "tests/ethernet_v1"))
from port_qemu import qualification


def table(body):
    raw = bytearray(
        struct.pack(
            "<4sIBB6s8sI4sI",
            b"DMAR",
            48 + len(body),
            1,
            0,
            b"NOVAOS",
            b"FICTIF5E",
            1,
            b"LAB5",
            1,
        )
        + bytes([47])
        + bytes(11)
        + body
    )
    raw[9] = (-sum(raw)) % 256
    return bytes(raw)


def dump(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("supports", type=Path)
    p.add_argument("firmware", type=Path)
    p.add_argument("sortie", type=Path)
    p.add_argument("--worktrees", type=Path, required=True)
    a = p.parse_args()
    supports, fw, out = map(Path.resolve, (a.supports, a.firmware, a.sortie))
    out.mkdir(parents=True, exist_ok=False)
    # Deux DRHD synthétiques sur segments distincts. Aucun registre n'est touché.
    drhd = lambda seg, addr: struct.pack("<HHBBHQ", 0, 16, 1, 0, seg, addr)
    two = out / "deux-drhd.aml"
    two.write_bytes(table(drhd(0, 0xFED90000) + drhd(1, 0xFEDA0000)))
    invalid = out / "invalide.aml"
    invalid.write_bytes(table(b"\0\0\x03\0"))
    cases = [
        ("direct", "direct", "diagnostic", "nominal", None, 0),
        ("pont", "pont", "diagnostic", "nominal", None, 0),
        ("deux-unites", "direct", "diagnostic", "nominal", two, 0),
        ("dmar-invalide", "direct", "diagnostic", "nominal", invalid, 5),
        ("dmar-double", "pont", "diagnostic", "nominal", two, 5),
        ("paquet-altere", "direct", "signature-interne", "nominal", None, 6),
        ("plancher", "direct", "epoque", "nominal", None, 12),
        ("origine-interne", "direct", "diagnostic", "nominal", None, 9),
        ("non-signe", "direct", "sans-signature", "nominal", None, None),
        ("substitution", "direct", "substitution", "nominal", None, None),
        ("noyau-altere", "direct", "noyau", "nominal", None, None),
        ("config-alteree", "direct", "configuration", "nominal", None, None),
        ("generation-revoquee", "direct", "ancienne", "nominal", None, None),
        ("signataire-revoque", "direct", "diagnostic", "revoque", None, None),
    ]
    result = dict(
        qemu=subprocess.check_output(
            ["qemu-system-x86_64", "--version"], text=True
        ).splitlines()[0],
        firmware=digest(fw),
        cas={},
        garde={},
    )
    try:
        with qualification(a.worktrees.resolve(), result["garde"]):
            for name, topo, image, store, fixture, status in cases:
                path = supports / (image + ".img")
                var = supports / (store + ".fd")
                args = dict(
                    media=path,
                    variables=var,
                    firmware=fw,
                    topology=topo,
                    table=fixture,
                    internal_boot=name == "origine-interne",
                )
                calibration = guest(out / (name + "-initial"), **args)
                if status is None:
                    run = calibration
                    fwlog = (out / (name + "-initial") / "firmware.log").read_bytes()
                    denied = any(
                        b"USB" in line
                        and (b"Access Denied" in line or b"Security Violation" in line)
                        for line in fwlog.splitlines()
                    )
                    run["refus_firmware_usb"] = denied
                    run["repli_interne_observe"] = run["rapport"] is not None
                    # Un repli vers notre témoin SATA doit lui-même s'arrêter avant inventaire.
                    # Son observation reste un échec de la porte globale même si les deux refus sont corrects.
                    if run["repli_interne_observe"]:
                        passed = (
                            denied
                            and run["rapport"]["statut"] == 9
                            and not run["rapport"]["noeuds"]
                        )
                    else:
                        passed = denied and b"No bootable option" in fwlog
                else:
                    if not calibration["rapport"]:
                        raise ValueError("Absence de rapport " + name)
                    run = guest(out / name, expected=calibration["rapport"], **args)
                    r = run["rapport"]
                    # Les callbacks OVMF de sortie peuvent réécrire à l'identique la commande USB.
                    # Vérifier le moment, le contrôleur et la valeur antérieure pour chaque écriture.
                    unchanged = True
                    for event in run["evenements_diagnostic"]:
                        match = re.fullmatch(
                            r"pci_cfg_write (qemu-xhci \S+) @0x4 <- (0x[0-9a-f]+)",
                            event,
                        )
                        if not match or run["commandes_pci_entree"].get(
                            match[1]
                        ) != int(match[2], 16):
                            unchanged = False
                    passed = bool(
                        r
                        and r["statut"] == status
                        and r["vivants"] == 0
                        and run["halt"]
                        and run["entree"] == run["fin"]
                        and unchanged
                        and run["octets_avant_sortie"] == run["octets_avant"]
                    )
                    if passed and status == 0:
                        kinds = {
                            n["type"]: (n["compatible"], n["plan"]) for n in r["noeuds"]
                        }
                        passed = (
                            kinds.get(1) == (1, 1)
                            and kinds.get(2) == (2, 0)
                            and kinds.get(3) == (0, 0)
                            and kinds.get(5) == (0, 0)
                            and all(n["activation"] == 0 for n in r["noeuds"])
                            and r["drhd"]
                            == {"direct": 0, "pont": 1, "deux-unites": 2}[name]
                        )
                    if passed and status != 0:
                        passed = not r["noeuds"]
                run["zero_io_interne_depuis_reset"] = all(
                    v == 0 for v in run["fin"].values()
                )
                run["attendu"] = status
                run["valide"] = passed and run["disque_intact"]
                result["cas"][name] = run
                dump(out / "bilan.json", result)
                print(
                    name,
                    run["valide"],
                    "statut",
                    run["rapport"] and run["rapport"]["statut"],
                    "lectures internes",
                    run["fin"]["rd_operations"],
                    flush=True,
                )
    finally:
        dump(out / "bilan.json", result)
    depths = {
        name: max(n["profondeur"] for n in result["cas"][name]["rapport"]["noeuds"])
        for name in ("direct", "pont")
    }
    if not depths["pont"] > depths["direct"]:
        raise ValueError("Chemin PCI derrière le pont non distingué")
    result["profondeur_pci"] = depths
    result["porte_physique"] = "bloquee"
    result["repli_observe"] = any(
        v.get("repli_interne_observe", False) for v in result["cas"].values()
    )
    dump(out / "bilan.json", result)
    if result["cas"]["direct"]["image"] != result["cas"]["pont"]["image"] or not all(
        x["valide"] for x in result["cas"].values()
    ):
        raise SystemExit("Critère de cellule non satisfait, consulter le bilan")


if __name__ == "__main__":
    main()
