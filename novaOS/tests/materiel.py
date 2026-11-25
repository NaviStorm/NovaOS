"""Compare l'inventaire de l'invité aux périphériques présents dans QEMU, sans disque cible."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

from demarrage import BUILD, ROOT, qmp


PCI = re.compile(
    r"\bPCI ([0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]) "
    r"([0-9a-f]{4}):([0-9a-f]{4}) classe=([0-9a-f]{6})\b"
    r"[^\r\n]*? etat=inventorie pilote=non-attribue", re.I,
)


def qmp_windows(roots):
    """Fenêtres inclusives configurées des ponts du banc."""
    names = {"io_range": "io", "memory_range": "memoire", "prefetchable_range": "prefetch"}
    expected = {}
    def visit(devices):
        for device in devices:
            bridge = device.get("pci_bridge")
            if not bridge:
                continue
            key = f"0000:{device['bus']:02x}:{device['slot']:02x}.{device['function']}"
            for source, name in names.items():
                span = bridge["bus"][source]
                if span["base"] <= span["limit"]:
                    expected[(key, name)] = (span["base"], span["limit"])
            visit(bridge.get("devices", []))
    for root in roots:
        visit(root["devices"])
    return expected


def qmp_inventory(roots):
    """query-pci rend la classe et la sous-classe, indépendamment de l'interface."""
    found = {}
    children = {}
    def visit(devices, parent):
        for device in devices:
            key = f"0000:{device['bus']:02x}:{device['slot']:02x}.{device['function']}"
            if key in found:
                raise RuntimeError(f"BDF répété dans QMP {key}")
            found[key] = (device["id"]["vendor"], device["id"]["device"], device["class_info"]["class"])
            name = device["qdev_id"]
            if name:
                children[name] = parent
            visit(device.get("pci_bridge", {}).get("devices", []), name)
    for root in roots:
        visit(root["devices"], None)
    if children.get("inventaire-enfant") != "inventaire-pont":
        raise RuntimeError("Le firmware n’a pas configuré la fonction témoin derrière le pont.")
    return found


def verify(text, roots, arch, acpi_off, granted):
    if "MATERIEL bilan ok=true" not in text:
        raise RuntimeError(f"Bilan du scénario en échec.\n{text[-2000:]}")
    level = "EL0" if arch == "aarch64" else "Ring3"
    actor = re.search(
        r"DEBUG pid=(\d+) MATERIEL gestionnaire inventaire lu; pilotes natifs actifs=0 "
        r"droits-materiel=aucun fonctions=0x([0-9a-f]{2})\b", text,
    )
    if not actor or not re.search(rf"PROC pid={actor[1]} niveau={level} pc=0x[0-9a-f]+ sortie=0\b", text):
        raise RuntimeError("Le gestionnaire ne termine pas avec succès au niveau de privilège utilisateur.")
    if acpi_off:
        if "ACPI absent dtb=true" not in text or "DTB ECAM verifie" not in text:
            raise RuntimeError("Le scénario ARM sans ACPI n’a pas exercé la voie de repli.")
    elif not all(f"ACPI table {tag}" in text for tag in ("APIC", "MCFG", "FACP")):
        raise RuntimeError(f"Tables firmware de référence absentes.\n{text[-2000:]}")
    found = {}
    for match in PCI.finditer(text):
        addr, vendor, device, pci_class = match.groups()
        key = addr.lower()
        if key in found:
            raise RuntimeError(f"BDF répété dans l'inventaire invité {key}")
        found[key] = (int(vendor, 16), int(device, 16), int(pci_class, 16) >> 8)
    expected = qmp_inventory(roots)
    if int(actor[2], 16) != len(found):
        raise RuntimeError("Le nombre de fonctions reçu en espace utilisateur diffère de la capture noyau.")
    if not granted:
        if found or "PCI acces non attribue" not in text:
            raise RuntimeError("Le profil générique doit refuser l’accès PCI et livrer zéro fonction.")
        return 0
    windows = {}
    for key, name, base, limit in re.findall(
        r"PCI-PONT (\S+) (io|memoire|prefetch)=(0x[0-9a-f]+)\.\.=(0x[0-9a-f]+)", text,
    ):
        if (key, name) in windows:
            raise RuntimeError(f"Fenêtre répétée {key} {name}")
        windows[(key, name)] = (int(base, 16), int(limit, 16))
    if windows != qmp_windows(roots):
        raise RuntimeError(f"Fenêtres PCI divergentes. QMP {qmp_windows(roots)}, invité {windows}")
    if expected != found:
        raise RuntimeError(f"PCI incomplet ou divergent. QMP {expected}, invité {found}")
    return len(found)


def run(arch, iso, acpi_off, granted, *, network=False, directory=None):
    name = f"{arch}-{'sans-acpi' if acpi_off else 'acpi'}"
    if not granted:
        name += "-sans-acces-pci"
    if directory is None:
        directory = Path(tempfile.mkdtemp(prefix=f"materiel-{name}-", dir=BUILD))
    serial, stderr = directory / "serie.log", directory / "qemu.log"
    args = [
        "sh", str(ROOT / "scripts/qemu.sh"), "-display", "none", "-monitor", "none",
        "-serial", f"file:{serial}", "-qmp", "stdio",
        "-device", "pcie-root-port,id=inventaire-pont,bus=pcie.0,addr=0a.0,chassis=1",
        "-device", "virtio-rng-pci,id=inventaire-enfant,bus=inventaire-pont,disable-legacy=on",
    ]
    if network:
        # Hub interne sans autre port, sans TAP, SLIRP ni interface hôte.
        args.extend(["-netdev", "hubport,id=netlab,hubid=17", "-device",
                     "virtio-net-pci,id=nic,netdev=netlab,addr=07.0,disable-legacy=on,romfile="])
    if acpi_off:
        args.extend(["-machine", "acpi=off"])
    env = {**os.environ, "NOVA_ARCH": arch, "NOVA_ISO": str(iso)}
    print(f"Preuves {directory}", flush=True)
    with stderr.open("wb") as errors, subprocess.Popen(
        args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors, bufsize=0, env=env,
    ) as vm:
        try:
            qmp(vm, "qmp_capabilities")
            version = qmp(vm, "query-version")
            until = time.monotonic() + 60
            text = ""
            while time.monotonic() < until and vm.poll() is None:
                text = serial.read_text(errors="replace") if serial.exists() else ""
                if "MATERIEL bilan" in text:
                    break
                if any(marker in text for marker in ("Panique du noyau", "Exception noyau", "#PF adresse", "ABORT adresse")):
                    break
                time.sleep(0.05)
            roots = qmp(vm, "query-pci")
            (directory / "qmp.json").write_text(json.dumps(roots, indent=2) + "\n")
            count = verify(text, roots, arch, acpi_off, granted)
            if network:
                nic = [dev for bus in roots for dev in bus["devices"] if dev["qdev_id"] == "nic"]
                if (len(nic) != 1 or nic[0]["slot"] != 7 or nic[0]["id"]["device"] != 0x1041
                        or nic[0]["id"]["vendor"] != 0x1af4 or nic[0]["class_info"]["class"] != 0x0200):
                    raise RuntimeError("Identité ou classe Ethernet inattendue dans QMP")
                marker = re.findall(r"DEBUG pid=(\d+) VirtIO-net PCI moderne inventorie", text)
                if len(marker) != 1 or f"DEBUG pid={marker[0]} MATERIEL gestionnaire inventaire lu" not in text:
                    raise RuntimeError("La carte doit être reconnue par le gestionnaire invité")
            qmp(vm, "quit")
            if vm.wait(timeout=5):
                raise RuntimeError(f"Arrêt anormal de QEMU, voir {stderr}")
        except Exception:
            shutil.copyfile(iso, directory / "echec.iso")
            raise
        finally:
            if vm.poll() is None:
                vm.kill()
                vm.wait()
    target = "aarch64-unknown-none-softfloat" if arch == "aarch64" else "x86_64-unknown-none"
    elf = Path(os.environ.get("NOVA_KERNEL_ELF", ROOT / "target" / target / "release/novaos"))
    result = {
        "architecture": arch, "acpi_desactive": acpi_off, "acces_pci": granted, "qemu": version,
        "peripheriques_compares": count, "sha256_iso": hashlib.sha256(iso.read_bytes()).hexdigest(),
        "sha256_elf": hashlib.sha256(elf.read_bytes()).hexdigest(),
        "sha256_qmp": hashlib.sha256((directory / "qmp.json").read_bytes()).hexdigest(),
        "sha256_serie": hashlib.sha256(serial.read_bytes()).hexdigest(),
    }
    (directory / "bilan.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"OK {name} — {count} fonctions admises, témoin en aval du pont vérifié via QMP.", flush=True)


def main():
    arch = os.environ["NOVA_ARCH"]
    if arch not in ("aarch64", "x86_64"):
        raise ValueError(arch)
    BUILD.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir=BUILD) as temp:
        iso = Path(temp) / "inventaire.iso"
        profiles = ["materiel-inventaire-qemu"]
        if arch == "x86_64":
            profiles.append("materiel-inventaire")
        for profile in profiles:
            subprocess.run(["sh", str(ROOT / "scripts/iso.sh"), profile, str(iso)],
                           check=True, stdout=subprocess.DEVNULL)
            granted = profile == "materiel-inventaire-qemu"
            run(arch, iso, False, granted)
            if arch == "aarch64":
                run(arch, iso, True, granted)


if __name__ == "__main__":
    main()
