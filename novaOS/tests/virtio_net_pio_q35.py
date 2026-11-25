"""Démarrage Q35 par PIO. Le support FAT de laboratoire et le NVMe sont distincts."""
import argparse
import os
from pathlib import Path
import re
import socket
import struct
import subprocess
import tempfile
import time

from ovmf_reseau_q35 import arguments, controle_vtd
from virtio_net_confinement import ROOT, capture, digest, dump, hmp, qmp, reg
from virtio_net_qemu import qualification
from virtio_net_relais_q35 import refus_liminer

SIZE = 34 << 20
PCI = {0: 0x29c08086, 48: 0x00101b36, 56: 0x10411af4, 248: 0x29188086, 251: 0x29308086}


def support_pio(raw, out):
    """Conserve les deux FAT et les fichiers du volume vvfat. Supprime seulement l'espace libre final."""
    with raw.open("rb") as src:
        mbr = src.read(512)
        start, count = struct.unpack_from("<II", mbr, 454)
        if start != 63 or mbr[450] != 6 or mbr[510:] != b"\x55\xaa":
            raise ValueError("Partition FAT16 attendue")
        src.seek(start * 512)
        image = bytearray(src.read(SIZE))
    if len(image) != SIZE or struct.unpack_from("<HBHBH", image, 11) != (512, 16, 1, 2, 512):
        raise ValueError("Géométrie FAT16 imprévue")
    if struct.unpack_from("<II", image, 28) != (start, count):
        raise ValueError("BPB et partition incompatibles")
    sectors_fat, = struct.unpack_from("<H", image, 22)
    length = sectors_fat * 512
    fat = image[512:512 + length]
    if fat != image[512 + length:512 + 2 * length]:
        raise ValueError("Les deux tables FAT divergent")
    clusters = (SIZE // 512 - 1 - 2 * sectors_fat - 32) // 16
    if not 4085 <= clusters <= 65524 or any(fat[(clusters + 2) * 2:]):
        raise ValueError("Une allocation FAT serait perdue")
    for word, in struct.iter_unpack("<H", fat[4:(clusters + 2) * 2]):
        if word and not (2 <= word <= clusters + 1 or word >= 0xfff8):
            raise ValueError("Chaîne FAT hors support compact")
    struct.pack_into("<II", image, 28, 0, SIZE // 512)
    with out.open("xb") as dest:
        dest.write(image)


def qt_command(qt, command):
    qt.write(command.encode() + b"\n")
    parts = qt.readline().split()
    if parts == [b"OK"]:
        return None
    if len(parts) == 2 and parts[0] == b"OK":
        return int(parts[1], 0)
    raise RuntimeError(f"Commande qtest {command} refusée, {parts!r}")


def pci_etat(qt, state):
    devices = {(dev["slot"] * 8 + dev["function"]):
               (dev["id"]["device"] << 16 | dev["id"]["vendor"])
               for bus in state["pci"] for dev in bus["devices"] if bus["bus"] == 0}
    if len(state["pci"]) != 1 or devices != PCI or state["vm"]["running"]:
        raise ValueError(f"Topologie Q35 PIO inattendue {devices}")
    saved = qt_command(qt, "inl 0xcf8")
    commands = {}
    try:
        for sid, identity in PCI.items():
            addr = 0x80000000 | sid << 8
            qt_command(qt, f"outl 0xcf8 {addr:#x}")
            if qt_command(qt, "inl 0xcfc") != identity:
                raise ValueError("Identité PCI instable")
            qt_command(qt, f"outl 0xcf8 {addr + 4:#x}")
            commands[sid] = qt_command(qt, "inw 0xcfc")
    finally:
        qt_command(qt, f"outl 0xcf8 {saved:#x}")
    if any(cmd & 4 for cmd in commands.values()):
        raise ValueError(f"Bus Master actif {commands}")
    return commands


def trace_fermee(out):
    writes = {sid: [] for sid in PCI}
    with (out / "qemu.trace").open() as stream:
        for number, line in enumerate(stream, 1):
            if not line.startswith("pci_cfg_write "):
                continue
            m = re.fullmatch(r"pci_cfg_write \S+ 00:([0-9a-f]+)\.(\d) @(0x[0-9a-f]+) <- (0x[0-9a-f]+)\n", line)
            if not m:
                raise ValueError(f"Écriture PCI non décodée ligne {number}")
            slot, function, offset, value = (int(v, 16) for v in m.groups())
            sid = slot * 8 + function
            if sid not in PCI or offset < 4:
                raise ValueError(f"Écriture PCI ambiguë ligne {number}")
            if offset == 4:
                writes[sid].append([number, value])
                if value & 4:
                    raise ValueError(f"Bus Master demandé pour SID {sid} ligne {number}")
    if not all(writes[sid] for sid in (48, 56)):
        raise ValueError("Trace PCI incomplète pour les deux domaines")
    return writes


def essai(firmware, entree, mode, out):
    out.mkdir()
    support_pio(entree / "support.raw", out / "boot.fat")
    disk = out / "namespace.raw"
    with disk.open("xb") as stream:
        stream.truncate(4 << 20)
    absent = mode == "sans-iommu"
    args = arguments(firmware / "OVMF_CODE.fd", entree, out, absent, acpi_pio=True)
    args[args.index("q35,smm=on")] = "q35,smm=on,sata=off"
    i = args.index("ide-hd,id=support,drive=source,bus=ide.0,bootindex=1")
    del args[i-3:i+1]
    # Un seul vCPU, sans hotplug. SMM reste obligatoire pour les variables authentifiées.
    for feature in ("broadcast", "cpu-hotplug", "cpu-hotunplug"):
        args.extend(["-global", f"ICH9-LPC.x-smi-{feature}=off"])
    args.extend(["-global", "fw_cfg_io.dma_enabled=off", "-fw_cfg", f"name=opt/novaos/boot.fat,file={out}/boot.fat",
                 "-device", "nvme,id=nv,bus=pcie.0,addr=06.0,serial=NOVA0190RW,max_ioqpairs=1,msix_qsize=2",
                 "-drive", f"if=none,id=stockage,format=raw,cache=writeback,file={disk}",
                 "-device", "nvme-ns,bus=nv,drive=stockage,nsid=1,shared=off,eui64=0x4e4f564130313930",
                 "-trace", "fw_cfg_select", "-trace", "vtd_inv_desc_wait_sw", "-trace", "vtd_inv_desc",
                 "-trace", "vtd_inv_desc_cc_device", "-trace", "vtd_inv_desc_iotlb_domain", "-no-shutdown"])
    dump(out / "arguments.json", args)
    report = {"cas": mode, "firmware_sha256": digest(firmware / "OVMF_CODE.fd"),
              "fat_sha256": digest(out / "boot.fat"), "variables_sha256": digest(entree / "variables.fd")}
    try:
        with qualification(ROOT.parent.parent, report), tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as temp:
            begin = time.monotonic()
            with (out / "qemu.trace").open("xb") as log, subprocess.Popen(
                    args, cwd=temp, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0) as vm:
                try:
                    qmp(vm, "qmp_capabilities")
                    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
                        sock.connect(str(Path(temp) / "qtest.sock"))
                        with sock.makefile("rwb", buffering=0) as qt:
                            initial = capture(vm, out, "reset", False, vtd=not absent)
                            report["reset_pci"] = pci_etat(qt, initial)
                            prop = qmp(vm, "qom-get", path="/machine/fw_cfg", property="dma_enabled")
                            if prop is not False or (not absent and initial["vtd_gsts"] != 0) or initial["iommu_platform"] is not True:
                                raise ValueError("Propriété DMA du fw_cfg ou VT-d initial")
                            for line in (out / "reset-mtree.txt").read_text().splitlines():
                                if "fwcfg.dma" in line:
                                    raise ValueError("Registre DMA du fw_cfg présent")
                            incompatible = mode in ("refus-ir", "refus-irta")
                            if incompatible:
                                expected = (0x2000000,0) if mode == "refus-ir" else (0,0x1000)
                            if mode == "refus-irta":
                                # Contrôle de refus uniquement. vCPU arrêté et tous les BME relus à zéro.
                                target = 0xfed90000
                                qt_command(qt, f"writeq {target+0xb8:#x} 0x1000")
                                observed = (reg(vm,target+0x1c), reg(vm,target+0xb8))
                                if observed != expected:
                                    raise ValueError(f"État de refus non établi matériellement {observed}")
                                report["etat_IR_avant_invite"] = observed
                            qmp(vm, "cont")
                            until = time.monotonic() + 35
                            while time.monotonic() < until:
                                debug = (out / "efi.log").read_text(errors="replace")
                                serial = (out / "serie.log").read_bytes()
                                if ("NOVA PIO terminal=" in debug or "NET VTD terminal " in debug or
                                    b"NET Q35 etat=" in serial or b"NET DUO etat=" in serial or refus_liminer(serial) or refus_liminer(debug.encode())):
                                    break
                                time.sleep(0.04)
                            qmp(vm, "stop")
                            (out / "registres-fin.txt").write_text(hmp(vm, "info registers"))
                            final = capture(vm, out, "fin", False, vtd=not absent)
                            if qmp(vm, "qom-get", path="/machine/fw_cfg", property="dma_enabled") is not False or "fwcfg.dma" in (out / "fin-mtree.txt").read_text():
                                raise ValueError("Accès DMA du fw_cfg en fin de démarrage")
                            report["fin_pci"] = pci_etat(qt, final)
                            debug = (out / "efi.log").read_text(errors="replace")
                            serial = (out / "serie.log").read_bytes()
                            if not all(debug.splitlines().count(tag) == 1 for tag in
                                       ("NOVA PIO fw_cfg=0000000000000001", "NOVA PIO bytes=0000000002200000",
                                        "NOVA PIO LoadImage=0000000000000000")):
                                raise ValueError("Relais PIO ou LoadImage non confirmé, " + debug[-240:])
                            if absent:
                                if debug.splitlines().count("NET VTD terminal dmar-refusee") != 1 or "terminal confine" in debug or b"NET DUO" in serial:
                                    raise ValueError("Refus EFI en absence de VT-d non établi")
                                report["refus_vtd"] = "dmar-refusee"
                            elif mode in ("signature", "dbx", "avant-dbx"):
                                expected = "0000000000000000" if mode == "avant-dbx" else "800000000000000f"
                                if debug.splitlines().count("NET EFI LoadImage refusee=" + expected) != 1:
                                    raise ValueError("Décision de vérification du second PE absente")
                                if mode != "avant-dbx":
                                    report["vtd_efi"] = controle_vtd(vm, qt, debug, final)
                            elif mode == "noyau-altere":
                                if not (refus_liminer(serial) or refus_liminer(debug.encode())) or b"NET Q35" in serial or final["vtd_gsts"] != 0:
                                    raise ValueError("Refus Limine non établi")
                            elif incompatible:
                                if not (b"NET DUO etat=Err(Feature)" in serial and b"DUO WAIT" not in serial and
                                        b"VTD actif" not in serial and final["vtd_gsts"] == expected[0] and
                                        reg(vm,0xfed900b8) == expected[1] and reg(vm,0xfed90020) == 0):
                                    raise ValueError("Refus avec racine vide non établi")
                                if mode == "refus-ir" and b"DUO temoin IRES=1 IRTA=0 BME=0 avant verification" not in serial:
                                    raise ValueError("IRES non observé à la frontière du superviseur")
                                report["refus_avant_attribution"] = True
                            elif mode == "topologie":
                                if b'NET Q35 etat=Err("topologie ou pairs")' not in serial or final["vtd_gsts"] != 0:
                                    raise ValueError("Témoin de refus par le noyau historique absent")
                            else:
                                from virtio_net_duo_tables import verifier
                                report["duo"] = verifier(vm, qt, serial, final, mode)
                    qmp(vm, "quit")
                    if vm.wait(timeout=5) != 0:
                        raise ValueError("Arrêt QEMU anormal")
                finally:
                    if vm.poll() is None:
                        vm.kill()
                    vm.wait()
                    report["execution_s"] = time.monotonic() - begin
            report["ecritures_pci"] = trace_fermee(out)
            report["reussi"] = True
    finally:
        dump(out / "campagne.json", report)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("firmware", type=Path)
    p.add_argument("entree", type=Path, help="FAT de référence et banque de variables UEFI enrôlée")
    p.add_argument("mode", choices=("signature", "dbx", "avant-dbx", "noyau-altere", "topologie", "sans-iommu",
                                   "domaines", "retrait-nic", "retrait-disque", "perte-wait", "total-nic", "total-disque", "refus-ir", "refus-irta"))
    p.add_argument("sortie", type=Path)
    a = p.parse_args()
    essai(a.firmware.resolve(), a.entree.resolve(), a.mode, a.sortie.resolve())
