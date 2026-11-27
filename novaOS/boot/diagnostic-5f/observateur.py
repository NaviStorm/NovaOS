"""Sondes du modèle QEMU arrêté. Aucun accès matériel depuis l'invité."""

import json
import re
import struct
from hashlib import sha256
from importlib.resources import files
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives.serialization import Encoding
from virt.firmware.efi.bootentry import BootEntry
from virt.firmware.efi.siglist import EfiSigDB
from virt.firmware.varstore.edk2 import Edk2VarStore


def sauver(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def variables(path):
    store = Edk2VarStore(path).get_varlist()

    def indices(key):
        if key not in store:
            return []
        return [i for (i,) in struct.iter_unpack("<H", store[key].data)]

    options = {}
    for name, var in store.items():
        if re.fullmatch(r"Boot[0-9A-Fa-f]{4}", name):
            entry = BootEntry(data=var.data)
            options[name] = {"titre": str(entry.title), "chemin": str(entry.devicepath)}
    return {
        "ordre": indices("BootOrder"),
        "prochain": indices("BootNext"),
        "options": options,
        "confiance": {
            k: sha256(store[k].data).hexdigest() for k in ("PK", "KEK", "db", "dbx")
        },
    }


USB, SATA = 0x51, 0x50


def magasin(source, output, chemins, ordre, prochain=None):
    """Seules les variables de sélection changent, sur une copie privée."""
    base = Edk2VarStore(source)
    vs = base.get_varlist()
    for k, dev in ((USB, "USB"), (SATA, "SATA")):
        vs.set_boot_entry(k, "LAB5F " + dev, chemins[dev])
    if "BootOrder" not in vs:
        vs.create("BootOrder")
    vs["BootOrder"].set_boot_order(ordre)
    if prochain is not None:
        vs.set_boot_next(prochain)
    base.write_varstore(output, vs)
    got = variables(output)
    if got["ordre"] != list(ordre) or got["prochain"] != (
        [] if prochain is None else [prochain]
    ):
        raise ValueError("Variables de démarrage différentes de la politique demandée")
    if got["confiance"] != variables(source)["confiance"]:
        raise ValueError("Variation des bases de confiance")


def chemin(store, transport):
    vs = Edk2VarStore(store).get_varlist()
    entries = [
        BootEntry(data=v.data)
        for k, v in vs.items()
        if re.fullmatch(r"Boot[0-9A-F]{4}", k)
    ]
    needle = "USB(" if transport == "USB" else "SATA("
    found = [e.devicepath for e in entries if needle in str(e.devicepath)]
    if len(found) != 1:
        raise ValueError(
            f"Chemin {transport} ambigu ou absent: {[str(e.devicepath) for e in entries]}"
        )
    return found[0]


def traductions(qmp, phase):
    """Observation du modèle Q35 arrêté via QMP. GSTS et PCI Command sont en lecture seule pour la sonde."""

    def hmp(command):
        return qmp.call("human-monitor-command", **{"command-line": command})

    mtree = hmp("info mtree").splitlines()

    def fenetres(model):
        lines = {x.strip() for x in mtree if x.strip().endswith(": " + model)}
        for line in sorted(lines):
            match = re.fullmatch(
                r"([0-9a-f]+)-([0-9a-f]+) \(prio [0-9]+, i/o\): " + model, line
            )
            if not match:
                raise ValueError("Fenêtre QEMU méconnaissable")
            yield line, int(match[1], 16), int(match[2], 16)

    def registre(base, end, offset, size):
        addr = base + offset
        if not (base <= addr <= end - size + 1):
            raise ValueError("Registre hors fenêtre")
        width = {2: "h", 4: "w"}[size]
        raw = hmp(f"xp /1{width}x {addr:#x}").strip()
        match = re.fullmatch(r"([0-9A-Fa-f]+):\s+0x([0-9A-Fa-f]+)", raw)
        if not match or int(match[1], 16) != addr or len(match[2]) != 2 * size:
            raise ValueError("Lecture de registre ambiguë: " + raw)
        return raw, int(match[2], 16)

    units = []
    for line, base, end in fenetres("intel_iommu"):
        raw, value = registre(base, end, 0x1C, 4)
        units.append({"fenetre": line, "lecture": raw, "tes": bool(value & (1 << 31))})
    sauver(Path("vtd-" + phase + ".json"), units)

    def devices(nodes):
        for dev in nodes:
            yield dev
            yield from devices(dev.get("pci_bridge", {}).get("devices", []))

    # Une seule fenêtre ECAM du Q35 simulé, segment et premier bus 0.
    ((window, base, end),) = fenetres("pcie-mmcfg-mmio")
    samples = {}
    for bus in qmp.call("query-pci"):
        for dev in devices(bus["devices"]):
            cls = dev["class_info"]["class"]
            if cls not in (0x0200, 0x0106):
                continue
            b, s, f = (dev[k] for k in ("bus", "slot", "function"))
            bdf = f"{b:02x}:{s:02x}.{f}"
            offset = (b << 20) + (s << 15) + (f << 12) + 4
            raw, value = registre(base, end, offset, 2)
            samples[bdf] = {
                "classe": cls,
                "lecture": raw,
                "commande": value,
                "bme": bool(value & 4),
            }
    if len(samples) != 2:
        raise ValueError("Contrôleur Ethernet ou SATA du témoin Q35 manquant")
    regs = hmp("info registers")
    rip = re.search(r"\bRIP=([0-9A-Fa-f]+)", regs)
    if not rip:
        raise ValueError("RIP du témoin absent")
    sauver(
        Path("pci-" + phase + ".json"),
        {"fenetre": window, "controleurs": samples, "rip": int(rip[1], 16)},
    )


def db_tierce(source, dest):
    """Construire hors ligne une fixture OVMF qui ne fait confiance qu’à la CA publique tierce pour les images."""
    cert = files("virt.firmware").joinpath("certs/microsoft.com/ms-uefi-2023.pem")
    der = x509.load_pem_x509_certificate(cert.read_bytes()).public_bytes(Encoding.DER)
    if (
        sha256(der).hexdigest()
        != "f6124e34125bee3fe6d79a574eaa7b91c0e7bd9d929c1a321178efd611dad901"
    ):
        raise ValueError("Certificat tiers de référence différent")
    base = Edk2VarStore(source)
    vs = base.get_varlist()
    vs.add_cert("db", "45434150-cdbe-4c63-8fcd-a45fa42834df", str(cert), replace=True)
    base.write_varstore(dest, vs)
    final = Edk2VarStore(dest).get_varlist()
    written = [item["data"] for group in EfiSigDB(final["db"].data) for item in group]
    if written != [der] or any(
        final[k].data != vs0.data for k, vs0 in base.get_varlist().items() if k != "db"
    ):
        raise ValueError("Magasin tiers différent du profil attendu")
