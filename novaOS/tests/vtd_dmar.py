"""Vérifie dans des VM neuves les refus précoces de la reprise VT-d/EDU sur Q35."""
import argparse
from contextlib import contextmanager
import json
import os
import re
import shutil
import select
from pathlib import Path
import struct
import subprocess
import tempfile
import time

from demarrage import ROOT, qmp
from materiel_pilote import read_config, sha256


def dmar_courte(path):
    # Checksum ACPI correct et en-tête de table complet. Le DRHD annonce
    # seulement 4 octets, en dessous des 16 octets minimaux de l'unité.
    blob = bytearray(52)
    struct.pack_into("<4sIB", blob, 0, b"DMAR", len(blob), 1)
    blob[10:24] = b"NOVA  " + b"DMARTEST"
    blob[36] = 47
    struct.pack_into("<HH", blob, 48, 0, 4)
    blob[9] = -sum(blob) & 255
    path.write_bytes(blob)


@contextmanager
def qtest(control):
    with control.with_suffix(".in").open("wb", buffering=0) as request, \
            control.with_suffix(".out").open("rb", buffering=0) as response:
        def q(command):
            request.write(command.encode() + b"\n")
            if not select.select([response], [], [], 5)[0]:
                raise TimeoutError(f"fixture QTest {command}")
            answer = response.readline().split()
            if not answer or answer[0] != b"OK":
                raise RuntimeError(f"fixture QTest {command} : {answer}")
            return int(answer[1], 0) if len(answer) > 1 else None
        yield q


def edu_utilisee(control):
    # Fixture avant firmware. RUN et BME à zéro durant la programmation.
    with qtest(control) as q:
        q("outl 0xcf8 0x80003000")
        if q("inl 0xcfc") != 0x11e81234:
            raise ValueError("EDU absente au slot 6")
        q("outl 0xcf8 0x80003004")
        if q("inw 0xcfc") != 0:
            raise ValueError("EDU déjà active avant le firmware")
        q("outl 0xcf8 0x80003010")
        if q("inl 0xcfc") != 0:
            raise ValueError("BAR0 préattribuée")
        q("outl 0xcfc 0x80000000")
        q("outl 0xcf8 0x80003004")
        q("outw 0xcfc 2")
        try:
            if q("readl 0x80000000") != 0x010000ed or q("readq 0x80000098") != 0:
                raise ValueError("EDU non vierge")
            q("writeq 0x80000090 1")
            if q("readq 0x80000090") != 1 or q("readq 0x80000098") != 0:
                raise ValueError("la fixture a soumis un DMA ou n'a pas armé le témoin")
        finally:
            q("outl 0xcf8 0x80003004")
            q("outw 0xcfc 0")
            q("outl 0xcf8 0x80003010")
            q("outl 0xcfc 0")


def pipe_pair(base):
    for suffix in (".in", ".out"):
        os.mkfifo(base.with_suffix(suffix), 0o600)


def stop_before_takeover(vm, debug, elf):
    # Le point d'arrêt de TCG place la fixture après ExitBootServices. Les deux
    # canaux se trouvent dans le répertoire temporaire privé de cette seule VM.
    symbols = subprocess.check_output(["llvm-nm", "--defined-only", "--demangle", elf], text=True)
    address, = re.findall(r"^([0-9a-f]+) t novaos::hardware::discovery::vtd::vtd_edu$", symbols, re.M)
    with debug.with_suffix(".in").open("wb", buffering=0) as tx, \
            debug.with_suffix(".out").open("rb", buffering=0) as rx:
        def byte():
            if not select.select([rx], [], [], 25)[0]:
                raise TimeoutError("GDB QEMU sans réponse")
            result = rx.read(1)
            if not result:
                raise EOFError("GDB QEMU fermé")
            return result

        def packet(command):
            payload = command.encode()
            tx.write(b"$" + payload + b"#" + f"{sum(payload) & 255:02x}".encode())
            if byte() != b"+" or byte() != b"$":
                raise ValueError("Réponse GDB QEMU invalide")
            reply = bytearray()
            while (value := byte()) != b"#":
                reply.extend(value)
                if len(reply) > 256:
                    raise ValueError("Réponse GDB QEMU excessive")
            if int(byte() + byte(), 16) != sum(reply) & 255:
                raise ValueError("Somme GDB QEMU invalide")
            tx.write(b"+")
            return bytes(reply)

        if packet("Z1," + address + ",1") != b"OK" or not packet("c").startswith((b"T05", b"S05")):
            raise ValueError("Arrêt à la reprise VT-d manquant")
        regs = qmp(vm, "human-monitor-command", **{"command-line": "info registers"})
        if not re.search(r"\bRIP=" + address + r"\b", regs, re.I):
            raise ValueError("Le BSP n'est pas à l'entrée de la reprise PCI")
        if packet("z1," + address + ",1") != b"OK":
            raise ValueError("Retrait du point d'arrêt")
    return address


def active_fixture(vm, name, control, debug, elf):
    address = stop_before_takeover(vm, debug, elf)
    with qtest(control) as q:
        if name == "ahci-tag":
            if read_config(vm, "x86_64", 0, 31, 2) != 0x29228086:
                raise ValueError("AHCI non identifié")
            if read_config(vm, "x86_64", 4, 31, 2) & 7 != 7:
                raise ValueError("Commande PCI AHCI initiale inattendue")
            abar = read_config(vm, "x86_64", 0x24, 31, 2)
            if abar != 0x80100000 or q(f"readl {abar + 0xc:#x}") != 0x3f:
                raise ValueError("BAR5 ou ports AHCI inattendus")
            ports = [abar + 0x100 + n*0x80 for n in range(6)]
            if any(q(f"readl {port + reg:#x}") for port in ports for reg in (0x34, 0x38)):
                raise ValueError("AHCI occupé avant la fixture")
            commands = [q(f"readl {port + 0x18:#x}") for port in ports]
            # Uniquement le tag logiciel sur le port sans média. Ne jamais
            # toucher PxCI (soumission effective d'une commande AHCI).
            q(f"writel {ports[-1] + 0x34:#x} 1")
            if q(f"readl {ports[-1] + 0x34:#x}") != 1 or q(f"readl {ports[-1] + 0x38:#x}") != 0:
                raise ValueError("Fixture SACT sans CI indisponible")
            return {"pc_reprise": address, "abar": abar, "commandes_ports": commands,
                    "port": 5, "sact": 1, "ci": 0}
        if read_config(vm, "x86_64", 0) != 0x11e81234 or read_config(vm, "x86_64", 4) & 7 != 7:
            raise ValueError("EDU ou BME hérité inattendu")
        base = read_config(vm, "x86_64", 0x10)
        if base != 0x80000000 or q(f"readl {base:#x}") != 0x010000ed:
            raise ValueError("BAR EDU non identifiée")
        if any(q(f"readq {base + off:#x}") for off in (0x80, 0x88, 0x90, 0x98)):
            raise ValueError("Moteur déjà programmé avant la fixture")
        # CMD=lecture, longueur nulle. Aucun octet de RAM n'est adressé.
        q(f"writeq {base + 0x88:#x} 0x40000")
        q(f"writeq {base + 0x98:#x} 1")
        count, run = (q(f"readq {base + off:#x}") for off in (0x90, 0x98))
        if count != 0 or run != 1:
            raise ValueError("Commande témoin en cours absente")
    return {"pc_reprise": address, "longueur": count, "cmd": run, "commande_pci": 7}


def read_root_port_acs(vm, slot):
    # Marche bornée des capacités étendues du seul port racine de la fixture.
    base = 0xe0000000 + (slot << 15)
    offset = 0x100
    visited = set()
    while offset:
        if not 0x100 <= offset <= 0xff8 or offset % 4 or offset in visited:
            raise ValueError("Chaîne de capacités PCIe invalide")
        visited.add(offset)
        def read(at):
            raw = qmp(vm, "human-monitor-command", **{"command-line": f"xp /1wx {base + at:#x}"})
            prefix, value = raw.strip().split(":")
            if int(prefix, 16) != base + at:
                raise ValueError("Adresse de relecture ECAM incohérente")
            return int(value, 16)
        header = read(offset)
        if header in (0, 0xffffffff):
            break
        if header & 0xffff == 0x000d:
            caps = read(offset + 4)
            return {"offset": offset, "capacites": caps & 0xffff, "controle": caps >> 16}
        offset = header >> 20
    return None


def vm_case(name, elf, iso, directory, env):
    capture = directory / name
    capture.mkdir()
    log = capture / "serie.log"
    devices = ["-device", "edu,id=edu-original,bus=pcie.0,addr=06.0"]
    if name in ("edu-multifonction", "groupe-partage"):
        devices[-1] += ",multifunction=on"
    if name == "dmar-malformee":
        table = capture / "malformee.bin"
        dmar_courte(table)
        devices += ["-acpitable", f"file={table}"]
        # Les autres profils possèdent une vraie unité émulée.
        # Ici la DMAR forgée est l'unique DMAR. Le refus doit précéder tout MMIO.
    else:
        devices += ["-device", "intel-iommu,intremap=off,aw-bits=48,device-iotlb=off"]
        if name == "topologie-etrangere":
            devices += ["-device", "edu,id=edu-etranger,bus=pcie.0,addr=07.0"]
        elif name == "groupe-partage":
            devices += ["-device", "edu,id=soeur,bus=pcie.0,addr=06.1"]
        elif name == "pont-pcie":
            devices += ["-device", "pcie-root-port,id=racine4,bus=pcie.0,addr=07.0",
                        "-device", "pcie-pci-bridge,id=pont,bus=racine4,addr=00.0",
                        "-device", "edu,id=aval,bus=pont,addr=01.0"]
        elif name in ("edu-utilisee", "edu-en-cours", "ahci-tag"):
            control = capture / "ctl"
            pipe_pair(control)
            devices += ["-S", "-qtest", f"pipe:{control}"]
            if name in ("edu-en-cours", "ahci-tag"):
                debug = capture / "debug"
                pipe_pair(debug)
                devices += ["-chardev", f"pipe,id=debug,path={debug}", "-gdb", "chardev:debug"]
        elif name == "dmar-double":
            table = capture / "double.bin"
            blob = bytearray(48)
            struct.pack_into("<4sIB", blob, 0, b"DMAR", 48, 1)
            blob[10:24] = b"NOVA  " + b"DMARTEST"
            blob[36] = 47
            blob[9] = -sum(blob) & 255
            table.write_bytes(blob)
            devices += ["-acpitable", f"file={table}"]
    cmd = ["sh", ROOT / "scripts/qemu.sh", "-display", "none", "-monitor", "none",
           "-qmp", "stdio", "-serial", f"file:{log}", *devices]
    with (capture / "qemu.log").open("wb") as errors, subprocess.Popen(
        cmd, env={**env, "NOVA_ISO": str(iso)}, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=errors, bufsize=0,
    ) as vm:
        try:
            qmp(vm, "qmp_capabilities")
            if name == "edu-utilisee":
                edu_utilisee(control)
                qmp(vm, "cont")
            if name in ("edu-en-cours", "ahci-tag"):
                fixture = active_fixture(vm, name, control, debug, elf)
                qmp(vm, "cont")
            deadline = time.monotonic() + 70
            while vm.poll() is None and time.monotonic() < deadline:
                serial = log.read_text(errors="replace") if log.exists() else ""
                if "VTD bilan ok=" in serial or "Panique du noyau" in serial:
                    break
                time.sleep(.04)
            else:
                raise RuntimeError(f"{name} sans bilan invité, voir {capture}")
            qmp(vm, "stop")
            pci = qmp(vm, "query-pci")
            ids = [d["qdev_id"] for bus in pci for d in bus["devices"]]
            if "edu-original" not in ids:
                raise ValueError(f"{name} sans carte EDU au slot 6")
            cmd_finale = read_config(vm, "x86_64", 4) & 0xffff
            if "Panique du noyau" in serial or 'VTD DMAR DRHD' in serial:
                raise ValueError(f"{name} a admis l'unité ou paniqué")
            if name == "topologie-etrangere":
                valid = ("edu-etranger" in ids and "VTD topologie PCI refusee" in serial
                         and "VTD bilan ok=false" in serial and "EDU PCI 0000:" not in serial
                         and cmd_finale == 7)
            elif name in ("edu-multifonction", "groupe-partage", "pont-pcie"):
                root = {d["qdev_id"]: d for bus in pci for d in bus["devices"]}
                if name == "pont-pcie":
                    bridge, = root["racine4"]["pci_bridge"]["devices"]
                    downstream, = bridge["pci_bridge"]["devices"]
                    shape = (bridge["qdev_id"], downstream["qdev_id"]) == ("pont", "aval")
                else:
                    shape = (read_config(vm, "x86_64", 0x0c) >> 16) & 0xff == 0x80
                    if name == "groupe-partage":
                        shape = shape and root["soeur"]["slot"] == 6 and root["soeur"]["function"] == 1
                valid = (shape and cmd_finale == 7 and "VTD bilan ok=false" in serial
                         and "EDU PCI 0000:" not in serial and "VTD temoin sans domaine" not in serial
                         and ("VTD groupe EDU multifonction refuse" if name == "edu-multifonction"
                              else "VTD topologie PCI refusee") in serial)
            elif name == "ahci-tag":
                ports = [fixture["abar"] + 0x100 + n*0x80 for n in range(6)]
                with qtest(control) as q:
                    after = [q(f"readl {port + 0x18:#x}") for port in ports]
                    retained = q(f"readl {ports[-1] + 0x34:#x}") == 1
                    empty = all(not q(f"readl {port + 0x38:#x}") for port in ports)
                fixture["commandes_ports_apres"] = after
                valid = ("VTD reprise AHCI refusee avant ecriture PCI" in serial
                         and cmd_finale == 7 and after == fixture["commandes_ports"] and retained and empty
                         and "VTD bilan ok=false" in serial and "EDU PCI 0000:" not in serial)
            elif name in ("edu-utilisee", "edu-en-cours"):
                expected_run = int(name == "edu-en-cours")
                valid = ("VTD reprise moteur EDU refusee avant ecriture PCI" in serial and cmd_finale == 7
                         and f"VTD EDU etat herite RUN={expected_run} vierge=false" in serial
                         and "EDU PCI 0000:" not in serial
                         and "VTD bilan ok=false" in serial and "VTD temoin sans domaine" not in serial
                         and "VTD EDU reprise moteur_vierge=true" not in serial)
            else:
                reason = "Length" if name == "dmar-malformee" else "Format"
                # QEMU injecte la table supplémentaire après la DMAR native.
                valid = (f"VTD ACPI refuse {reason}" in serial and cmd_finale & 7 == 0
                         and "VTD EDU reprise moteur_vierge=true BME=0" in serial
                         and "VTD temoin sans domaine ring3_refuse=true pci_ferme=true" in serial
                         and "VTD bilan ok=true" in serial)
            if not valid:
                raise ValueError(f"{name} refus ou restitution manquants, PCI={cmd_finale:#06x}, {capture}\n{serial[-1200:]}")
            result = {"profil": name, "qemu": qmp(vm, "query-version"), "pci": pci,
                      "commande_edu_finale": cmd_finale, "sha256_serie": sha256(log),
                      "sha256_elf": sha256(elf), "sha256_iso": sha256(iso),
                      "serie": log.read_bytes().decode("utf-8", errors="replace")}
            if name in ("edu-en-cours", "ahci-tag"):
                result["fixture"] = fixture
            if name == "pont-pcie":
                result["acs_port"] = read_root_port_acs(vm, 7)
            if name in ("dmar-malformee", "dmar-double"):
                result["dmar_injectee"] = table.read_bytes().hex()
            if name != "dmar-malformee":
                # Le remappeur existe dans ces témoins.
                # Ni son RTADDR ni son enable ne peuvent provenir du pilote refusé.
                for register, address in (("gsts", 0xfed9001c), ("root_lo", 0xfed90020),
                                          ("root_hi", 0xfed90024)):
                    text = qmp(vm, "human-monitor-command",
                               **{"command-line": f"xp /1wx {address:#x}"})
                    m = re.fullmatch(r"\s*([0-9a-fA-F]+):\s*0x([0-9a-fA-F]+)\s*", text)
                    if not m or int(m[1], 16) != address:
                        raise ValueError(f"relecture du registre {register} impossible : {text}")
                    result[register] = int(m[2], 16)
                if result["gsts"] & (1 << 31) or result["root_lo"] or result["root_hi"]:
                    raise ValueError(f"{name} a installé un remappage malgré son refus")
            peers = {f"00:{slot:02x}.{fn}": read_config(vm, "x86_64", 4, slot, fn) & 0xffff
                     for slot, fn in ((0, 0), (31, 0), (31, 2), (31, 3))}
            expected_peer = 3 if name in ("dmar-malformee", "dmar-double") else 7
            if set(peers.values()) != {expected_peer}:
                raise ValueError(f"{name} commande PCI des pairs inattendue : {peers}")
            result["pci_pairs"] = peers
            qmp(vm, "quit")
            if vm.wait(timeout=5) != 0:
                raise RuntimeError(f"arrêt anormal de {name}")
        finally:
            if vm.poll() is None:
                vm.kill()
                vm.wait()
    print(f"OK {name}, PCI_COMMAND EDU={cmd_finale:#06x}", flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", required=True, type=Path)
    parser.add_argument("--preuves", required=True, type=Path)
    args = parser.parse_args()
    source = args.elf.resolve(strict=True)
    directory = args.preuves.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="nova-vtd-", dir="/tmp") as tmp:
        elf = Path(tmp) / "noyau.elf"
        shutil.copyfile(source, elf)
        env = {**os.environ, "NOVA_ARCH": "x86_64", "NOVA_VCPUS": "1", "NOVA_VGA": "none",
               "NOVA_KERNEL_ELF": str(elf)}
        iso = Path(tmp) / "diagnostic.iso"
        subprocess.run(["sh", ROOT / "scripts/iso.sh", "vtd-edu-test", iso], check=True,
                       env=env, stdout=subprocess.DEVNULL)
        records = [vm_case(case, elf, iso, directory, env)
                   for case in ("dmar-malformee", "dmar-double", "topologie-etrangere", "edu-utilisee",
                                "edu-multifonction", "groupe-partage", "pont-pcie", "edu-en-cours", "ahci-tag")]
    (directory / "bilan.json").write_text(json.dumps(records, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
