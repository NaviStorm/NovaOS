"""Observateur du démarrage USB. Entrée et HLT du même PE, média fourni par descripteur."""

import contextlib
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import time

from support import OS, PE_SHA, empreinte, public_member, require

for sub in ("diagnostic-5e", "diagnostic-5m"):
    sys.path.insert(0, str(OS / "boot" / sub))
from banc import Debug, Qmp
from image import pe_info, sections_chargees

FIRMWARE = "32807682a9e5c0e2d192ecb6077941d4be8bcfa6b9cd6e60830c3a9f48c0555a"
STORES = {
    "actif": "dd0e83543b732a6b0497e63f106fb064c25b0034591c84d471e585d901a0d671",
    "inactif": "8c09b326b45951f539619b4d0f33e5e8fdbafa1cca9b173468f9df7e4a7725e0",
}
HORS_DB = "1995f6f368ddc088500602a5fa4e13f9bd2325b24f4cf9d6122307e488266909"
MAGIC = b"NOVA5M-RAM-FINAL"


def ram(q, base, size):
    p = Path("prive-ram.bin")
    require(not p.exists(), "Résidu RAM privé")
    q.call("pmemsave", val=base, size=size, filename=str(p))
    try:
        return p.read_bytes()
    finally:
        p.unlink()


def count(q):
    rows = q.call("query-blockstats")
    return {
        r["device"]: {
            k: r["stats"][k]
            for k in (
                "rd_operations",
                "wr_operations",
                "flush_operations",
                "unmap_operations",
                "rd_bytes",
                "wr_bytes",
            )
        }
        for r in rows
        if r.get("device") in ("usb5n", "interne")
    }


def record(data, base):
    locations = [
        m.start()
        for m in re.finditer(MAGIC, data)
        if m.start() + 104 <= len(data)
        and struct.unpack_from("<Q", data, m.start() + 16)[0] == base + m.start()
    ]
    require(len(locations) == 1, "Verdict RAM absent ou ambigu")
    pos = locations[0]
    _, _, status, trust, arena, live, epoch, image = struct.unpack_from(
        "<16s5Q16s32s", data, pos
    )
    require(
        status == 16
        and trust == 2
        and live == 0
        and epoch == bytes(16)
        and image == bytes(32),
        "Verdict de refus USB 5M inattendu",
    )
    return dict(statut=status, confiance=trust, arena=arena, vivants=live)


@contextlib.contextmanager
def directory(p):
    old = Path.cwd()
    os.chdir(p)
    try:
        yield
    finally:
        os.chdir(old)


def boot(folder, fw, mode, medium, base=None, duplicate=None, temoin=False):
    folder.mkdir(mode=0o700)
    pe = (
        public_member("hors-db.efi", HORS_DB)
        if temoin
        else public_member("valide.efi", PE_SHA)
    )
    meta = pe_info(pe)
    require(empreinte(fw.read_bytes()) == FIRMWARE, "Firmware hors profil")
    with directory(folder):
        Path("variables.fd").write_bytes(public_member(mode + ".fd", STORES[mode]))
        canary = bytes(range(256)) * (4096 * 4)
        Path("interne.raw").write_bytes(canary)
        args = [
            "qemu-system-x86_64",
            "-machine",
            "q35,smm=on",
            "-accel",
            "tcg,thread=single",
            "-cpu",
            "max",
            "-m",
            "128M",
            "-smp",
            "1",
            "-nodefaults",
            "-display",
            "none",
            "-serial",
            "file:firmware.log",
            "-monitor",
            "none",
            "-qmp",
            "stdio",
            "-gdb",
            "unix:observateur.sock,server=on,wait=off",
            "-S",
            "-no-reboot",
            "-no-shutdown",
            "-boot",
            "strict=on",
            "-nic",
            "none",
            "-drive",
            "if=pflash,unit=0,readonly=on,format=raw,file=" + str(fw),
            "-drive",
            "if=pflash,unit=1,format=raw,file=variables.fd",
            "-global",
            "driver=cfi.pflash01,property=secure,value=on",
            "-device",
            "intel-iommu",
            "-device",
            "qemu-xhci,id=xhci",
            "-drive",
            "if=none,id=interne,format=raw,file=interne.raw",
            "-device",
            "ide-hd,id=sata,drive=interne,bus=ide.0",
            "-D",
            "transactions.log",
        ]
        files = [medium] if duplicate is None else [medium, duplicate]
        fds = []
        for index, f in enumerate(files, 1):
            fd, name = f.fileno(), ("usb5n" if index == 1 else "clone5n")
            fds.append(fd)
            args.extend(
                [
                    "-add-fd",
                    f"fd={fd},set={index},opaque={name}",
                    "-drive",
                    f"if=none,id={name},format=raw,readonly=on,file.driver=file,file.filename=/dev/fdset/{index},file.locking=off",
                    "-device",
                    f"usb-storage,id=cle{index},port={index},bus=xhci.0,drive={name}"
                    + (",bootindex=1" if index == 1 else ""),
                ]
            )
        with (
            open("qemu.log", "wb") as err,
            subprocess.Popen(
                args,
                pass_fds=fds,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=err,
                bufsize=0,
            ) as proc,
        ):
            dbg = None
            try:
                q = Qmp(proc)
                reset = count(q)
                rows = q.call("query-block")
                usb = next(r for r in rows if r["device"] == "usb5n")
                require(
                    usb["inserted"]["ro"]
                    and usb["inserted"]["file"] == "/dev/fdset/1"
                    and "cle1" in usb["qdev"],
                    "Liaison QMP du support",
                )
                sets = q.call("query-fdsets")
                require(
                    sum(any(f.get("opaque") == "usb5n" for f in s["fds"]) for s in sets)
                    == 1,
                    "Origine du descripteur ambiguë",
                )
                board = q.call("query-pci")
                classes = sorted(
                    d["class_info"]["class"] for bus in board for d in bus["devices"]
                )
                if temoin and mode == "actif":
                    q.call("cont")
                    deadline = time.monotonic() + 35
                    while (
                        b"No bootable option" not in Path("firmware.log").read_bytes()
                    ):
                        require(time.monotonic() < deadline, "Refus UEFI non terminal")
                        time.sleep(0.05)
                    q.call("stop")
                    fwlog = Path("firmware.log").read_bytes()
                    require(
                        b"Access Denied" in fwlog and b"BdsDxe: starting" not in fwlog,
                        "Repli ou absence de refus firmware",
                    )
                    result = dict(
                        mode=mode,
                        temoin=HORS_DB,
                        decision="EFI_ACCESS_DENIED",
                        lancement=False,
                        compteurs_terminal=count(q),
                    )
                elif base is None:
                    q.call("cont")
                    end = time.monotonic() + 35
                    while b"BdsDxe: starting" not in Path("firmware.log").read_bytes():
                        require(time.monotonic() < end, "Aucun lancement USB")
                        time.sleep(0.05)
                    time.sleep(0.5)
                    q.call("stop")
                    data = ram(q, 0, 128 << 20)
                    record(data, 0)
                    rva, offset, sz = meta["texte"]
                    signature = re.escape(pe[offset : offset + 64])
                    candidates = [
                        m.start() - rva
                        for m in re.finditer(signature, data)
                        if m.start() >= rva
                        and m.start() % 4096 == 0
                        and data[m.start() - rva : m.start() - rva + 2] == b"MZ"
                    ]
                    require(len(candidates) == 1, "Calibration ambiguë")
                    result = {"base": candidates[0]}
                else:
                    entry, halt = base + meta["entree"], base + meta["hlt"]
                    dbg = Debug()
                    dbg.watch(entry)
                    dbg.watch(halt)
                    dbg.resume()
                    require(dbg.pc() == entry, "Premier RIP")
                    dbg.remove(entry)
                    before = count(q)
                    sections = sections_chargees(pe, base)
                    for name, (rva, size, expected) in sections.items():
                        require(
                            empreinte(ram(q, base + rva, size)) == expected,
                            "Section PE " + name,
                        )
                    for event in ("ide_bus_exec_cmd", "ahci_cmd_done"):
                        q.call("trace-event-set-state", name=event, enable=True)
                    cut = Path("transactions.log").stat().st_size
                    dbg.resume()
                    require(dbg.pc() == halt, "RIP terminal")
                    rec = record(ram(q, base, meta["taille"]), base)
                    after = count(q)
                    trace = Path("transactions.log").read_bytes()[cut:]
                    require(
                        not trace and before == after, "Transaction EFI vers un disque"
                    )
                    lines = [
                        s.split("BdsDxe: ", 1)[1]
                        for s in Path("firmware.log")
                        .read_text(errors="replace")
                        .splitlines()
                        if "BdsDxe: starting" in s
                    ]
                    require(
                        len(lines) == 1
                        and "QEMU USB HARDDRIVE" in lines[0]
                        and "/USB(0x0,0x0)" in lines[0],
                        "Démarrage autre que le port USB attendu",
                    )
                    require(
                        before["usb5n"]["rd_bytes"] > 0
                        and before["usb5n"]["wr_bytes"] == 0,
                        "Activité du support USB",
                    )
                    result = dict(
                        mode_banc=mode,
                        origine=lines[0],
                        supports_usb=len(files),
                        fdset=usb["inserted"]["file"],
                        lecture_seule=True,
                        pe=empreinte(pe),
                        sections_verifiees=len(sections),
                        entree=entry,
                        hlt=halt,
                        compteurs_reset=reset,
                        compteurs_entree=before,
                        compteurs_hlt=after,
                        trace_commandes_sata_octets=len(trace),
                        inventaire_classes_pci=classes,
                        verdict=rec,
                        dma_precoce_usb="indeterminee",
                    )
            finally:
                # Le débogueur reste connecté jusqu'à la fin du processus arrêté.
                proc.terminate()
                proc.wait(timeout=10)
                if dbg:
                    dbg.sock.close()
        require(Path("interne.raw").read_bytes() == canary, "Altération SATA interne")
        result.update(vm_arretee=True, interne_intact=True)
        return result
