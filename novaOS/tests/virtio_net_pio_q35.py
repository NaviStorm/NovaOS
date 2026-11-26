"""Démarrage Q35 par PIO. Le support FAT de laboratoire et le NVMe sont distincts."""
import argparse
from contextlib import nullcontext
import json
import os
from pathlib import Path
import re
import shutil
import socket
import struct
import subprocess
import tempfile
import time

from ovmf_reseau_q35 import arguments, controle_vtd
from virtio_net_confinement import ROOT, capture, digest, dump, hmp, qmp, reg
from virtio_net_qemu import qualification
from virtio_net_relais_q35 import WAIT_CONTENTION, refus_liminer

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


def essai(firmware, entree, mode, out, sector=None, previous=None, reader=None):
    out.mkdir()
    report = {"cas": mode, "reussi": False}
    try:
        with qualification(ROOT.parent.parent, report):
            _essai_sous_verrou(firmware, entree, mode, out, sector, previous, reader, report)
    finally:
        dump(out / "campagne.json", report)


def _essai_sous_verrou(firmware, entree, mode, out, sector, previous, reader, report):
    injection = os.environ.get("NOVAOS_Q35_WAIT")
    contention_wait=mode in WAIT_CONTENTION
    if contention_wait:
        from virtio_net_duo_attente import verifier_preconditions
        if injection!=mode.split("-")[3]:
            raise ValueError("Injection différente du scénario de contention")
        verifier_preconditions(firmware,entree,mode)
    elif (mode.startswith("wait-") or injection is not None) and mode != "wait-" + str(injection):
        raise ValueError("Le modèle de faute et le scénario signé doivent correspondre")
    if os.environ.get("NOVAOS_Q35_REVUE_WAIT") or injection:
        closed = {"signature","avant-dbx","dbx","noyau-altere","sans-iommu","domaines","total-nic",
                  "wait-absent","wait-tardif","wait-court"}
        if mode not in closed and not contention_wait:
            raise ValueError("Ce binaire de faute est réservé aux scénarios à maîtres fermés")
    storage = mode.startswith("nvme-")
    traffic = mode.startswith(("nvme-contention-","nvme-panne-")) or mode in ("nvme-charge", "nvme-charge-disque","nvme-mesure-charge", "nvme-cycles")
    if storage and (sector not in (512, 4096) or reader is None or (mode == "nvme-init") != (previous is None)):
        raise ValueError("Géométrie, lecteur ou image source du cycle NovaFS absente")
    support_pio(entree / "support.raw", out / "boot.fat")
    disk = out / "namespace.raw"
    if storage:
        from nvme_phase2_oracle import inspect, virgin
        if previous is None:
            virgin(disk)
        else:
            shutil.copyfile(previous, disk)
            old = inspect(reader, disk, sector)
            if old["etat"] != ("ancien" if mode != "nvme-recover" else "nouveau"):
                raise ValueError("État initial NovaFS inattendu")
    else:
        with disk.open("xb") as stream:
            stream.truncate(4 << 20)
    initial_disk = digest(disk)
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
    if storage:
        for event in ("vtd_reg_write","vtd_reg_ir_root","vtd_ir_enable"):
            args.extend(["-trace",event])
        pos = next(i for i, a in enumerate(args) if a.startswith("nvme-ns,"))
        args[pos] += f",id=ns,logical_block_size={sector},physical_block_size={sector}"
        if mode == "nvme-recover":
            pos = next(i for i, a in enumerate(args) if a.startswith("if=none,id=stockage,"))
            args[pos] += ",readonly=on"
        for event in ("pci_nvme_mmio_cfg", "pci_nvme_admin_cmd", "pci_nvme_io_cmd", "pci_nvme_enqueue_req_completion",
                      "pci_nvme_read", "pci_nvme_write", "pci_nvme_flush_ns", "pci_nvme_err_req_status"):
            args += ["-trace", event]
    if traffic:
        # Ici le NIC est le seul périphérique à virtqueues. Les traces de complétion
        # et celles des CQE NVMe utilisent le même ordre du processus QEMU.
        args.extend(["-trace","virtqueue_fill","-trace","vtd_dmar_fault"])
    dump(out / "arguments.json", args)
    report.update({"cas": mode, "firmware_sha256": digest(firmware / "OVMF_CODE.fd"),
              "fat_sha256": digest(out / "boot.fat"), "variables_sha256": digest(entree / "variables.fd")})
    try:
        from virtio_net_pair import Pair
        with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as temp, (Pair(out, contention=mode.startswith("nvme-contention-"), panne_nic=mode=="nvme-panne-nic") if traffic else nullcontext(None)) as pair:
            fds = pair.connecter(args) if pair else ()
            dump(out / "arguments.json", args)
            begin = time.monotonic()
            with (out / "qemu.trace").open("xb") as log, subprocess.Popen(
                    args, cwd=temp, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0, pass_fds=fds) as vm:
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
                            if storage:
                                ns = [qmp(vm, "qom-get", path="/machine/peripheral/ns", property=k)
                                      for k in ("logical_block_size", "physical_block_size")]
                                drive, = [d["inserted"] for d in qmp(vm,"query-block") if d.get("device")=="stockage"]
                                if ns != [sector,sector] or drive["ro"] != (mode=="nvme-recover") or drive["cache"] != {"writeback":True,"direct":False,"no-flush":False}:
                                    raise ValueError("Droits, cache ou géométrie NVMe imprévus")
                            qmp(vm, "cont")
                            until = time.monotonic() + (300 if traffic or mode=="nvme-mesure" else 35)
                            while time.monotonic() < until:
                                debug = (out / "efi.log").read_text(errors="replace")
                                serial = (out / "serie.log").read_bytes()
                                if ("NOVA PIO terminal=" in debug or "NET VTD terminal " in debug or
                                    b"Panique du noyau" in serial or b"NET Q35 etat=" in serial or b"NET DUO etat=" in serial or b"NVME bilan ok=" in serial or refus_liminer(serial) or refus_liminer(debug.encode())):
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
                            elif storage:
                                incertain=contention_wait and injection!="court"
                                retirement = (b"DUO panne NVMe BME=0 DMA_retenu=6 NIC_verifie=true" if mode=="nvme-panne-disque"
                                              else b" BME=0 contexte=0 IOTLB=drain WAIT=confirme ")
                                verdict=b"NVME bilan ok=true"
                                if incertain:
                                    verdict=b"NVME bilan ok=false"
                                    retirement=b"DUO contention deux_retraits_refuses "
                                if (final["vtd_gsts"] != 0xc4000000 or reg(vm,0xfed90034) != 0 or
                                    reg(vm,0xfed900b8) != 0 or reg(vm,0xfed900bc) != 0 or
                                    verdict not in serial or retirement not in serial):
                                    raise ValueError("Issue du stockage ou retrait incompatible : " + serial[-900:].decode(errors="replace"))
                                if contention_wait:
                                    from virtio_net_duo_attente import registres
                                    report["registres_wait"]=registres(vm,qt,serial,injection,mode)
                                if mode.startswith("nvme-panne-"):
                                    from virtio_net_duo_pannes import tables_finales
                                    report["quarantaine"]=tables_finales(vm,qt,serial,mode)
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
            if storage:
                from virtio_net_duo_stockage import verifier
                if contention_wait:
                    from virtio_net_duo_attente import verifier_contention
                    report["stockage"]=verifier_contention(out,mode,sector,reader,initial_disk,old,injection)
                elif mode.startswith("nvme-panne-"):
                    from virtio_net_duo_pannes import verifier_panne
                    report["stockage"] = verifier_panne(out, mode, sector, reader, previous, old)
                else:
                    report["stockage"] = verifier(out, mode, sector, reader, initial_disk, old if previous else None)
            else:
                report["ecritures_pci"] = trace_fermee(out)
                if injection:
                    trace = (out/"qemu.trace").read_text()
                    addr = report["duo"]["registres"][0x90]
                    markers = re.findall(r"NOVA WAIT hold mode=(\w+) cookie=4 head=11 addr=(0x[0-9a-f]+)",trace)
                    writes = re.findall(r"vtd_inv_desc_wait_sw wait invalidate status write addr (0x[0-9a-f]+) data (0x[0-9a-f]+)",trace)
                    cookies = [int(v,16) for _,v in writes]
                    released = trace.count("NOVA WAIT release cookie=4 head=11")
                    if (len(markers)!=1 or markers[0][0]!=injection or not writes or
                        {p for p,_ in writes} != {markers[0][1]} or int(markers[0][1],16)==addr or
                        cookies != ([1,2,3] if injection=="absent" else [1,2,3,4]) or
                        released != int(injection!="absent")):
                        raise ValueError("Suspension ou écriture matérielle du WAIT non confirmée")
                    report["faute_wait"] = dict(mode=injection, marqueurs=markers, cookies=cookies, reprises=released)
        if traffic and not json.loads((out / "pair.json").read_text())["succes"]:
            raise ValueError("Chronologie ou intégrité Ethernet incomplète sur le pair")
        if mode == "nvme-cycles":
            pair = json.loads((out/"pair.json").read_text())
            if pair["generations"] != [1,2,3,4] or (pair["trames_rx"],pair["trames_tx"]) != (192,192):
                raise ValueError("Quatre générations Ethernet distinctes non confirmées au pair")
        report["reussi"] = True
    finally:
        dump(out / "campagne.json", report)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("firmware", type=Path)
    p.add_argument("entree", type=Path, help="FAT de référence et banque de variables UEFI enrôlée")
    p.add_argument("mode", choices=WAIT_CONTENTION + ("signature", "dbx", "avant-dbx", "noyau-altere", "topologie", "sans-iommu",
                                   "domaines", "ir-retrait", "retrait-nic", "retrait-disque", "perte-wait", "wait-court", "wait-tardif", "wait-absent", "nvme-cycles", "nvme-contention-nic", "nvme-contention-disque", "nvme-panne-nic", "nvme-panne-disque", "total-nic", "total-disque", "refus-ir", "refus-irta", "nvme-init", "nvme-update", "nvme-recover", "nvme-charge", "nvme-charge-disque","nvme-mesure","nvme-mesure-charge"))
    p.add_argument("sortie", type=Path)
    p.add_argument("--secteur",type=int,choices=(512,4096))
    p.add_argument("--volume",type=Path)
    p.add_argument("--lecteur",type=Path)
    a = p.parse_args()
    essai(a.firmware.resolve(), a.entree.resolve(), a.mode, a.sortie.resolve(), a.secteur, a.volume, a.lecteur)
