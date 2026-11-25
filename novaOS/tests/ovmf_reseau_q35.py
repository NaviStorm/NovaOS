"""Compare deux firmwares OVMF sur Q35, NIC isolé et support EFI synthétique."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import tempfile
import time

from virtio_net_confinement import ROOT, capture, digest, dump, qmp, reg
from virtio_net_qemu import qualification

CMD_WRITE = re.compile(r"^pci_cfg_write virtio-net-pci 00:07\.0 @(0x[0-9a-f]+) <- (0x[0-9a-f]+)$")


def pci_lecture(vm, stream):
    if qmp(vm, "query-status")["running"]:
        raise ValueError("Lecture CF8/CFC avec vCPU en mouvement")

    def io(command, read=False):
        stream.write(command.encode() + b"\n")
        reply = stream.readline().strip().split()
        if read and len(reply) == 2 and reply[0] == b"OK":
            return int(reply[1], 0)
        if not read and reply == [b"OK"]:
            return None
        raise ValueError(f"Réponse qtest incorrecte pour {command} : {reply}")

    saved = io("inl 0xcf8", True)
    try:
        # Seul le sélecteur de configuration du pont est écrit.
        io("outl 0xcf8 0x80003800")
        if io("inl 0xcfc", True) != 0x10411af4:
            raise ValueError("NIC absent au slot de qualification")
        io("outl 0xcf8 0x80003804")
        return io("inw 0xcfc", True)
    finally:
        io(f"outl 0xcf8 {saved:#x}")
        if io("inl 0xcf8", True) != saved:
            raise RuntimeError("Sélecteur PCI non restitué")


def commandes_pci(trace):
    writes = []
    for index, line in enumerate(trace.splitlines(), 1):
        if not line.startswith("pci_cfg_write virtio-net-pci "):
            continue
        m = CMD_WRITE.fullmatch(line)
        if not m:
            raise ValueError(f"Format ou identité PCI inattendu à la ligne {index}")
        offset, value = (int(n, 16) for n in m.groups())
        if 0 < offset < 4:
            raise ValueError(f"Largeur de configuration ambiguë à la ligne {index}")
        if offset == 0:
            raise ValueError("Écriture non prévue dans l'identité PCI")
        if offset == 4:
            writes.append({"ligne": index, "valeur": value})
    if not writes:
        raise ValueError("Aucune écriture de commande du NIC observée")
    return writes


def analyser(trace, debug):
    writes = commandes_pci(trace)
    pages = re.findall(r"^NET EFI pages boot=([\da-f]{16}) runtime=([\da-f]{16}) libres=([\da-f]{16})$", debug, re.M)
    if len(pages) != 1:
        raise ValueError("Trace des commandes PCI ou bilan mémoire incomplet")
    if debug.splitlines().count("NET EFI SecureBoot=1 SetupMode=0") != 1 or \
       debug.splitlines().count("NET EFI LoadImage admise=0000000000000000") != 1 or \
       debug.splitlines().count("NET EFI LoadImage refusee=800000000000000f") != 1 or \
       debug.splitlines().count("NET EFI controle fini") != 1:
        raise ValueError("Preuve de confiance EFI incomplète")
    return {"ecritures_commande": writes, "demandes_bme": sum(bool(w["valeur"] & 4) for w in writes),
            "pages_uefi": dict(zip(("boot", "runtime", "libres"), (int(n, 16) for n in pages[0]))),
            "secure_boot": 1, "setup_mode": 0, "load_image_refuse": "0x800000000000000f"}


def controle_vtd(vm, qt, journal, fin):
    m = re.search(r"^NET VTD root=([0-9a-f]{16}) gsts=00000000c4000000 recu=0000000000000001$", journal, re.M)
    if not m or "NET VTD terminal confine\n" not in journal:
        raise ValueError("Activation non établie : " + journal[-280:])
    root = int(m[1], 16)
    if not 0 < root <= 256 * 1024**2 - 12288 or root % 4096:
        raise ValueError("Racine de laboratoire hors géométrie")
    if fin["vtd_gsts"] != 0xc4000000 or reg(vm, 0xfed90020) != root or reg(vm, 0xfed90024) != 0 or reg(vm, 0xfed90034):
        raise ValueError("État du DRHD en désaccord avec la sonde EFI")
    qt.write(f"read {root:#x} 4096\n".encode())
    if qt.readline().strip() != b"OK 0x" + b"00" * 4096:
        raise ValueError("Racine matérielle non vide ou illisible")
    if (reg(vm, 0xfed90080), reg(vm, 0xfed90088), reg(vm, 0xfed90090),
        reg(vm, 0xfed90094), reg(vm, root + 8192)) != (48, 48, root + 4096, 0, 1):
        raise ValueError("Acquittement QI non corroboré au point d'arrêt")
    return {"octets_racine_nulle_relus": 4096, "gsts": fin["vtd_gsts"], "fsts": 0,
            "recu_qi": 1, "cadres_reserves_terminal": 3}


def essai(nom, code, temoin, out, terminal, absent, temoin_dbx):
    out.mkdir()
    args = [shutil.which("qemu-system-x86_64"), "-machine", "q35,smm=on", "-accel", "tcg",
            "-cpu", "max", "-smp", "1", "-m", "256", "-nodefaults", "-no-reboot", "-S",
            "-display", "none", "-monitor", "none", "-serial", f"file:{out}/serie.log",
            "-debugcon", f"file:{out}/efi.log", "-global", "isa-debugcon.iobase=0xe9",
            "-global", "driver=cfi.pflash01,property=secure,value=on", "-qmp", "stdio",
            "-qtest", "unix:qtest.sock,server=on,wait=off", "-qtest-log", "/dev/null",
            "-drive", f"if=pflash,unit=0,format=raw,readonly=on,file={code}",
            "-drive", f"if=pflash,unit=1,format=raw,snapshot=on,file={temoin}/variables.fd",
            "-drive", f"if=none,id=source,format=raw,snapshot=on,file={temoin}/support.raw",
            "-device", "ide-hd,id=support,drive=source,bus=ide.0,bootindex=1",
            "-nic", "none", "-netdev", "hubport,id=prive,hubid=17", "-device",
            "virtio-net-pci,id=nic,addr=7,netdev=prive,iommu_platform=on,ats=off,disable-legacy=on,vectors=0,romfile="]
    if not absent:
        args += ["-device", "intel-iommu,id=i,intremap=on,eim=on,aw-bits=48,caching-mode=on,device-iotlb=off"]
    for event in ("pci_cfg_write", "vtd_dmar_enable", "vtd_reg_write_gcmd", "vtd_reg_dmar_root"):
        args += ["-trace", event]
    dump(out / "arguments.json", args)
    start = time.monotonic()
    rss = []
    with (out / "qemu.trace").open("xb") as log, subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0) as vm:
        try:
            qmp(vm, "qmp_capabilities")
            dump(out / "version.json", qmp(vm, "query-version"))
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
                sock.settimeout(4)
                sock.connect("qtest.sock")
                with sock.makefile("rwb", buffering=0) as qt:
                    reset = capture(vm, out, "reset", False, vtd=not absent)
                    cmd_reset = pci_lecture(vm, qt)
                    if cmd_reset & 4 or reset.get("vtd_gsts", 0) != 0 or reset["iommu_platform"] is not True:
                        raise ValueError("État initial de la fixture inattendu")
                    qmp(vm, "cont")
                    until = time.monotonic() + 40
                    while time.monotonic() < until:
                        rss.append(int(subprocess.check_output(["ps", "-o", "rss=", "-p", str(vm.pid)])))
                        marker = b"NET VTD terminal " if terminal else b"NET EFI controle fini"
                        if temoin_dbx:
                            marker = b"NET EFI LoadImage refusee="
                        journal = (out / "efi.log").read_bytes()
                        if any(line.startswith(marker) for line in journal.splitlines(keepends=True) if line.endswith(b"\n")):
                            break
                        time.sleep(0.2)
                    qmp(vm, "stop")
                    fin = capture(vm, out, "fin", False, vtd=not absent)
                    cmd_fin = pci_lecture(vm, qt)
                    if fin["pci_command"] != cmd_fin:
                        raise ValueError("Désaccord entre ECAM et CF8")
                    dump(out / "pci.json", {"reset": cmd_reset, "fin": cmd_fin})
                    if absent:
                        journal = (out / "efi.log").read_text().splitlines()
                        if journal.count("NET VTD terminal dmar-refusee") != 1 or any("terminal confine" in l for l in journal):
                            raise ValueError("Refus invité faute de DMAR non établi")
                        vtd = {"statut": "dmar-refusee", "bail_attribue": False}
                    elif terminal:
                        vtd = controle_vtd(vm, qt, (out / "efi.log").read_text(), fin)
            qmp(vm, "quit")
            if vm.wait(timeout=5):
                raise RuntimeError("Sortie QEMU anormale")
        finally:
            if vm.poll() is None:
                vm.terminate()
                try:
                    vm.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    vm.kill()
                    vm.wait()
            dump(out / "duree.json", {"execution_s": time.monotonic() - start, "rss_hote_Kio": rss})
    trace = (out / "qemu.trace").read_text()
    debug = (out / "efi.log").read_text()
    if temoin_dbx:
        sequence = ["NET EFI SecureBoot=1 SetupMode=0", "NET EFI LoadImage admise=0000000000000000",
                    "NET EFI LoadImage refusee=0000000000000000"]
        if debug.splitlines()[:3] != sequence:
            raise ValueError("Le même second PE signé doit être admis avant sa révocation dans dbx")
        writes = commandes_pci(trace)
        obs = {"ecritures_commande": writes, "demandes_bme": sum(bool(w["valeur"] & 4) for w in writes),
               "secure_boot": 1, "setup_mode": 0, "chargement_second_pe_avant_dbx": "0x0"}
    else:
        obs = analyser(trace, debug)
    obs.update(commande_reset=cmd_reset, commande_fin=cmd_fin, gsts_fin=fin.get("vtd_gsts"),
               rss_hote_pic_echantillonne_Kio=max(rss), bail_attribue=False)
    if terminal:
        obs["terminal_vtd"] = vtd
    dump(out / "resultat.json", obs)
    if nom == "variante" and (obs["demandes_bme"] or cmd_fin & 4):
        raise ValueError("Le firmware réservé a laissé armer le NIC")
    return obs


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("firmwares", type=Path)
    p.add_argument("temoin", type=Path)
    p.add_argument("sortie", type=Path)
    modes = p.add_mutually_exclusive_group()
    modes.add_argument("--controle-dbx", action="store_true", help="contrôle positif du second PE avant inscription dans dbx")
    modes.add_argument("--sans-iommu", action="store_true", help="contrôle négatif de la même image terminale")
    modes.add_argument("--vtd", action="store_true", help="variante seule avec le témoin de racine fermée")
    args = p.parse_args()
    firmwares, temoin, out = (v.resolve() for v in (args.firmwares, args.temoin, args.sortie))
    out.mkdir()
    terminal = args.vtd or args.sans_iommu
    report = {"resultats": {}}
    dump(out / "entrees.json", {str(f.relative_to(firmwares)): digest(f) for f in (
        firmwares / "amont/OVMF_CODE.fd", firmwares / "variante/OVMF_CODE.fd", temoin / "variables.fd",
        temoin / "support.raw", temoin / "esp/EFI/BOOT/BOOTX64.EFI", temoin / "esp/EFI/BOOT/REFUSE.EFI")})
    try:
        with qualification(ROOT.parent.parent, report):
            for nom in (("variante",) if terminal or args.controle_dbx else ("amont", "variante")):
                with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"], prefix="q35-net") as tmp:
                    prev = os.getcwd()
                    try:
                        os.chdir(tmp)  # Chemin relatif pour le socket Unix sur Darwin.
                        report["resultats"][nom] = essai(nom, firmwares / nom / "OVMF_CODE.fd", temoin, out / nom, terminal, args.sans_iommu, args.controle_dbx)
                    finally:
                        os.chdir(prev)
    finally:
        dump(out / "campagne.json", report)


if __name__ == "__main__":
    def interrompre(_signum, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrompre)
    main()
