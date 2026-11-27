"""Campagne 5F sur les octets publiés de 5E et des magasins OVMF privés."""

import argparse
import json
import re
import subprocess
import sys
from collections import Counter
from importlib.metadata import version
from pathlib import Path

from observateur import (
    SATA,
    USB,
    chemin,
    db_tierce,
    magasin,
    sauver,
    traductions,
    variables,
)

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5e"))
sys.path.insert(0, str(NOVA / "tests/ethernet_v1"))
from banc import digest, guest
from port_qemu import qualification

IMAGE = "746553f2d540153a355235143c57ed8d0d6af42f46a9178f58848f4848407d99"
BASE = "c8a792f2d9e1cce2f315261fdda88778897f46b9"
EXTRA = ("usb_xhci_xfer_success",)


def exiger(ok, message):
    if not ok:
        raise ValueError(message)


def observer_cas(
    root,
    name,
    topo,
    image,
    source,
    fw,
    *,
    statut,
    denied=False,
    order=False,
    option=None,
    disk=False,
    usb=True,
):
    dest = root / name
    params = {
        "media": image,
        "variables": source,
        "firmware": fw,
        "topology": topo,
        "internal_boot": disk,
        "firmware_order": order,
        "usb_connected": usb,
        "observe": traductions,
        "extra_events": EXTRA,
    }
    # Même état initial pour la calibration d'adresse et la mesure. Aucun réglage du code.
    if statut is not None:
        first = guest(root / (name + "-cal"), **params)
        exiger(
            first["rapport"] and first["rapport"]["statut"] == statut,
            "Calibration " + name,
        )
        run = guest(dest, expected=first["rapport"], **params)
    else:
        run = guest(dest, **params)
    log = (dest / "firmware.log").read_text(errors="replace")
    trace = (dest / "transactions.log").read_bytes()

    def compteurs(data):
        return dict(
            Counter(
                line.split(None, 1)[0].decode() for line in data.splitlines() if line
            )
        )

    run["usb_connecte"] = usb
    run["avant"] = variables(source)
    run["apres"] = variables(dest / "variables.fd")
    run["magasin_final"] = digest(dest / "variables.fd")
    exiger(run["avant"]["confiance"] == run["apres"]["confiance"], "Bases modifiées")
    exiger(not run["apres"]["prochain"], "BootNext non consommé")
    entry = run["entree"]
    total = run["fin"]
    exiger(not any(run["initial"].values()), "Compteurs initiaux non nuls")
    exiger(
        run["disque_intact"]
        and all(v == 0 for k, v in total.items() if not k.startswith("rd_")),
        "Mutation SATA",
    )
    usb_denied = any(
        "USB" in line and ("Access Denied" in line or "Security Violation" in line)
        for line in log.splitlines()
    )
    exiger(denied == usb_denied, "Décision Secure Boot USB")
    if option is not None:
        exiger(f"starting Boot{option:04X} " in log, "Boot#### lancé inattendu")
    run["refus_usb"] = usb_denied
    run["vtd"] = {}
    run["pci"] = {}
    for phase in ["entree", "sortie", "fin"] if entry is not None else ["fin"]:
        rows = json.loads((dest / ("vtd-" + phase + ".json")).read_text())
        exiger(len(rows) == int(topo == "pont"), "Fenêtres VT-d inattendues")
        run["vtd"][phase] = rows
        run["pci"][phase] = json.loads((dest / ("pci-" + phase + ".json")).read_text())
    if statut is None:
        exiger(
            run["rapport"] is None
            and entry is None
            and "No bootable option" in log
            and total["rd_operations"] > 0,
            "Rejet terminal OVMF",
        )
        run["trace_avant"] = compteurs(trace)
    else:
        report = run["rapport"]
        exiger(
            report
            and report["statut"] == statut
            and run["halt"]
            and report["vivants"] == 0,
            "Rapport ou arrêt invalide",
        )
        exiger(
            not any(x["activation"] for x in report["noeuds"])
            and (statut == 0 or not report["noeuds"]),
            "Publication ou activation inattendue",
        )
        exiger(entry is not None and entry == total, "E/S SATA après entrée")
        cut = run["octets_avant"]
        exiger(run["octets_avant_sortie"] == cut, "Événement avant ExitBootServices")
        run["trace_avant"] = compteurs(trace[:cut])
        run["trace_apres"] = compteurs(trace[cut:])
        for event in run["evenements_diagnostic"]:
            g = re.fullmatch(
                r"pci_cfg_write (qemu-xhci \S+) @0x4 <- (0x[0-9a-f]+)", event
            )
            exiger(
                g and run["commandes_pci_entree"].get(g[1]) == int(g[2], 16),
                "Événement après entrée: " + event,
            )
        run["bme_herite"] = {
            k: bool(v & 4)
            for k, v in run["commandes_pci_entree"].items()
            if k.startswith(("e1000e ", "ich9-ahci "))
        }
        exiger(
            len(run["bme_herite"]) == 2 and all(run["bme_herite"].values()),
            "Témoin BME manquant",
        )
        for model, enabled in run["bme_herite"].items():
            sample = run["pci"]["entree"]["controleurs"][model.split()[1]]
            exiger(sample["bme"] == enabled, "Relecture PCI Command divergente")
        for phase, address in (
            ("entree", report["entree"]),
            ("sortie", report["sortie_efi"]),
            ("fin", run["rip_final"]),
        ):
            exiger(run["pci"][phase]["rip"] == address, "Point de mesure RIP divergent")
        if statut == 9:
            exiger(entry["rd_operations"] > 0, "Témoin d'origine interne")
    exiger(
        bool(run["trace_avant"].get("usb_xhci_xfer_success", 0)) == usb,
        "Témoin de transfert USB incohérent",
    )
    print(
        f"{name:28} statut={statut} SATA={total['rd_operations']} octets={total['rd_bytes']}",
        flush=True,
    )
    return run


def serie(topo, out, supports, nominal, revoked, tiers, fw, result):
    root = out / topo
    root.mkdir()
    cases = result["cas"][topo] = {}

    def run(name, image="diagnostic", store=nominal, **kw):
        cases[name] = observer_cas(
            root, name, topo, supports / (image + ".img"), store, fw, **kw
        )
        sauver(out / "bilan.json", result)

    run("amovible", statut=0)
    run("sata", statut=9, disk=True)
    paths = {
        dev: chemin(root / name / "variables.fd", dev)
        for dev, name in (("USB", "amovible"), ("SATA", "sata"))
    }
    for name, ordre, nxt in (
        ("ordre-usb", (USB, SATA), None),
        ("ordre-sata", (SATA, USB), None),
        ("prochain-usb", (SATA, USB), USB),
        ("prochain-absent", (SATA, USB), 0x5F),
    ):
        target = root / (name + ".fd")
        magasin(nominal, target, paths, ordre, nxt)
        chosen = USB if name in ("ordre-usb", "prochain-usb") else SATA
        run(
            name,
            store=target,
            statut=0 if chosen == USB else 9,
            order=True,
            option=chosen,
        )
    run(
        "demarrage-suivant",
        store=root / "prochain-usb/variables.fd",
        statut=9,
        order=True,
        option=SATA,
    )
    run(
        "prochain-usb-refuse",
        image="substitution",
        store=root / "prochain-usb.fd",
        statut=9,
        order=True,
        option=SATA,
        denied=True,
    )
    run(
        "support-absent",
        store=root / "prochain-usb.fd",
        statut=9,
        order=True,
        option=SATA,
        usb=False,
    )
    for name in (
        "sans-signature",
        "substitution",
        "configuration",
        "noyau",
        "ancienne",
    ):
        run(name, image=name, statut=9, denied=True)
    run("db-tierce", store=tiers, statut=None, denied=True)
    run("signataire-revoque", store=revoked, statut=None, denied=True)
    run("signature-interne", image="signature-interne", statut=6)
    run("epoque", image="epoque", statut=12)
    exiger(
        cases["demarrage-suivant"]["magasin"] == cases["prochain-usb"]["magasin_final"],
        "Le même magasin doit survivre au redémarrage",
    )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "lot", type=Path, help="archive 5E décompressée, avec supports/diagnostic.img"
    )
    p.add_argument("sortie", type=Path, help="nouveau répertoire privé")
    p.add_argument("--worktrees", type=Path, required=True)
    a = p.parse_args()
    lab, out = a.lot.resolve(), a.sortie.resolve()
    supports, fw = lab / "supports", lab / "ovmf/code.fd"
    nominal, revoked = supports / "nominal.fd", supports / "revoque.fd"
    provenance = json.loads(
        (NOVA / "docs/validation/diagnostic-uefi-5e/bilan.json").read_text()
    )
    exiger(
        digest(lab / "SHA256SUMS")
        == "19dd04f63dc16cfe0f9ed2171fe1f0eabc7c9b224ff79bad1376b0edfd9e2e08"
        and digest(fw) == provenance["firmware"]
        and digest(supports / "diagnostic.img") == IMAGE,
        "Témoin 5E différent",
    )
    for line in (lab / "SHA256SUMS").read_text().splitlines():
        h, filename = line.split(maxsplit=1)
        exiger(digest(lab / filename) == h, "Membre 5E différent: " + filename)
    out.mkdir(parents=True, exist_ok=False)
    result = {
        "base": BASE,
        "qemu": subprocess.check_output(
            ["qemu-system-x86_64", "--version"], text=True
        ).splitlines()[0],
        "virt_firmware": version("virt-firmware"),
        "firmware": digest(fw),
        "image": IMAGE,
        "garde": {},
        "cas": {},
    }
    tiers = out / "db-tierce.fd"
    db_tierce(nominal, tiers)
    try:
        with qualification(a.worktrees.resolve(), result["garde"]):
            for topo in ("direct", "pont"):
                serie(topo, out, supports, nominal, revoked, tiers, fw, result)
    finally:
        sauver(out / "bilan.json", result)


if __name__ == "__main__":
    main()
