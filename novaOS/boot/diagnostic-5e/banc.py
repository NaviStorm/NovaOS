"""Observateur hors invité. Les seuls périphériques sont des modèles QEMU et des fichiers fictifs."""

import re
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import time
from hashlib import sha256

MAGIC = b"NOVA5E-RAM-FINAL"
RAM = 128 * 1024 * 1024
EVENTS = (
    "pci_cfg_write",
    "e1000e_core_write",
    "vtd_reg_write",
    "ide_bus_exec_cmd",
    "e1000e_tx_descr",
    "e1000e_rx_descr",
    "e1000e_irq_msi_notify",
    "e1000e_irq_legacy_notify",
    "e1000e_irq_msix_notify_vec",
    "vtd_iotlb_page_hit",
    "vtd_iotlb_page_update",
    "ahci_cmd_done",
)


def digest(p):
    return sha256(p.read_bytes()).hexdigest()


class Qmp:
    def __init__(self, proc):
        self.proc = proc
        self.read()
        self.call("qmp_capabilities")

    def read(self):
        import select

        if not select.select([self.proc.stdout], [], [], 20)[0]:
            raise TimeoutError("QMP")
        line = self.proc.stdout.readline()
        if not line:
            raise RuntimeError("QEMU fermé, consulter qemu.log")
        return json.loads(line)

    def call(self, cmd, **arguments):
        self.proc.stdin.write(
            json.dumps(dict(execute=cmd, arguments=arguments)).encode() + b"\n"
        )
        self.proc.stdin.flush()
        while True:
            msg = self.read()
            if "error" in msg:
                raise RuntimeError(msg["error"])
            if "return" in msg:
                return msg["return"]

    def stats(self):
        rows = self.call("query-blockstats")
        with Path("compteurs-qmp.jsonl").open("a") as stream:
            stream.write(json.dumps(rows) + "\n")
        row = next(x for x in rows if x.get("device") == "interne")
        return {
            k: v
            for k, v in row["stats"].items()
            if k.endswith(("_operations", "_bytes"))
        }

    def events(self, on, extra=()):
        for name in EVENTS + extra:
            self.call("trace-event-set-state", name=name, enable=on)


class Debug:
    """Points d'arrêt d'exécution TCG. Aucune instruction ni donnée écrite dans l'invité."""

    def __init__(self):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.settimeout(35)
        self.sock.connect("observateur.sock")
        self.command("?")

    def command(self, txt):
        b = txt.encode()
        self.sock.sendall(b"$" + b + b"#" + f"{sum(b) & 255:02x}".encode())
        if self.sock.recv(1) != b"+":
            raise RuntimeError("Acquittement GDB")
        if self.sock.recv(1) != b"$":
            raise RuntimeError("Paquet GDB invalide")
        payload = bytearray()
        while True:
            c = self.sock.recv(1)
            if c == b"#":
                break
            if not c:
                raise EOFError("GDB")
            payload += c
        checksum = self.sock.recv(2, socket.MSG_WAITALL)
        if int(checksum, 16) != sum(payload) & 255:
            raise ValueError("Checksum GDB")
        self.sock.sendall(b"+")
        return payload.decode()

    def watch(self, address):
        if self.command(f"Z1,{address:x},1") != "OK":
            raise RuntimeError("Point d'arrêt")

    def pc(self):
        regs = bytes.fromhex(self.command("g"))
        return int.from_bytes(regs[128:136], "little")

    def remove(self, address):
        if self.command(f"z1,{address:x},1") != "OK":
            raise RuntimeError("Retrait du point d’arrêt")

    def resume(self):
        if not self.command("c").startswith(("T05", "S05")):
            raise RuntimeError("Arrêt invité inattendu")


def snapshot(q):
    q.call("pmemsave", val=0, size=RAM, filename="memoire.bin")
    data = Path("memoire.bin").read_bytes()
    Path("memoire.bin").unlink()
    pos = 0
    while True:
        pos = data.find(MAGIC, pos)
        if pos < 0:
            return None
        v = struct.unpack_from("<12Q", data, pos + 16)
        if v[0] == pos and 0 < v[1] < RAM and 0 < v[2] < RAM and v[8] <= 64:
            Path("rapport-ram.bin").write_bytes(data[pos : pos + 112 + 64 * 72])
            nodes = []
            for i in range(v[8]):
                kind, candidate, plan, activation, h = struct.unpack_from(
                    "<4B32s", data, pos + 112 + i * 72
                )
                depth, dev, fn, _, location = struct.unpack_from(
                    "<4B32s", data, pos + 112 + i * 72 + 36
                )
                nodes.append(
                    dict(
                        profondeur=depth,
                        devfn=[dev, fn] if depth else None,
                        chemin=location.hex() if depth else None,
                        type=kind,
                        compatible=candidate,
                        plan=plan,
                        activation=activation,
                        manifeste=h.hex() if plan else None,
                    )
                )
            return dict(
                adresse=pos,
                entree=v[1],
                arret=v[2],
                base=v[3],
                statut=v[4],
                tables=v[5],
                dmar=v[6],
                drhd=v[7],
                noeuds=nodes,
                arena=v[9],
                vivants=v[10],
                sortie_efi=v[11],
            )
        pos += 1


def guest(
    folder,
    media,
    variables,
    firmware,
    topology,
    expected=None,
    table=None,
    internal_boot=False,
    firmware_order=False,
    usb_connected=True,
    observe=None,
    extra_events=(),
):
    for source in (media, variables, firmware, media.parent / "diagnostic.img"):
        if not source.is_file():
            raise ValueError("Le banc exige des fichiers ordinaires")
    folder.mkdir()
    origin = Path.cwd()
    os.chdir(folder)
    try:
        shutil.copyfile(variables, "variables.fd")
        shutil.copyfile(media.parent / "diagnostic.img", "interne.raw")
        internal_hash = digest(Path("interne.raw"))
        args = [
            "qemu-system-x86_64",
            "-machine",
            "q35,smm=on",
            "-accel",
            "tcg,thread=single",
            "-cpu",
            "max",
            "-smp",
            "1",
            "-m",
            "128M",
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
            *([] if firmware_order else ["-boot", "strict=on"]),
            "-drive",
            f"if=pflash,unit=0,readonly=on,format=raw,file={str(firmware).replace(chr(44), chr(44) * 2)}",
            "-drive",
            "if=pflash,unit=1,format=raw,file=variables.fd",
            "-global",
            "driver=cfi.pflash01,property=secure,value=on",
            "-device",
            "qemu-xhci,id=usb",
        ]
        if usb_connected:
            args += [
                "-drive",
                f"if=none,id=externe,format=raw,readonly=on,file={str(media).replace(chr(44), chr(44) * 2)}",
                "-device",
                "usb-storage,bus=usb.0,drive=externe"
                + ("" if internal_boot or firmware_order else ",bootindex=1"),
            ]
        args += [
            "-drive",
            "if=none,id=interne,format=raw,file=interne.raw",
            "-device",
            "ide-hd,drive=interne,bus=ide.0"
            + (",bootindex=1" if internal_boot and not firmware_order else ""),
            "-netdev",
            "hubport,hubid=0,id=isole",
            "-D",
            "transactions.log",
        ]
        if topology == "pont":
            args += [
                "-device",
                "intel-iommu",
                "-device",
                "pcie-root-port,id=pont,slot=4,chassis=4",
            ]
        args += [
            "-device",
            "e1000e,netdev=isole,romfile="
            + (",bus=pont" if topology == "pont" else ""),
        ]
        if table:
            args += ["-acpitable", "file=" + str(table)]
        with (
            open("qemu.log", "wb") as errors,
            subprocess.Popen(
                args,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=errors,
                bufsize=0,
            ) as proc,
        ):
            try:
                q = Qmp(proc)
                start = q.stats()
                q.events(True, extra_events)
                boundary = None
                rip = None
                handoff = None
                if expected:
                    debug = Debug()
                    debug.watch(expected["entree"])
                    debug.watch(expected["arret"])
                    debug.watch(expected["sortie_efi"])
                    debug.resume()
                    if debug.pc() != expected["entree"]:
                        raise RuntimeError("RIP entrée")
                    debug.remove(expected["entree"])
                    before = q.stats()
                    if observe:
                        observe(q, "entree")
                    boundary = Path("transactions.log").stat().st_size
                    debug.resume()
                    if debug.pc() != expected["sortie_efi"]:
                        raise RuntimeError("RIP sortie des services EFI")
                    debug.remove(expected["sortie_efi"])
                    handoff = Path("transactions.log").stat().st_size
                    if observe:
                        observe(q, "sortie")
                    debug.resume()
                    rip = debug.pc()
                    if rip != expected["arret"]:
                        raise RuntimeError(
                            f"RIP arrêt {rip:#x} attendu {expected['arret']:#x}"
                        )
                    debug.remove(expected["arret"])
                    debug.sock.close()
                    q.call("cont")
                    time.sleep(0.25)
                    q.call("stop")
                    after = q.stats()
                else:
                    q.call("cont")
                    # Arrêt de l’application (HLT avec IF à zéro) ou décision terminale de BDS.
                    deadline = time.monotonic() + 35
                    while time.monotonic() < deadline:
                        boot_log = Path("firmware.log").read_bytes()
                        if b"No bootable option" in boot_log:
                            break
                        if b"BdsDxe:" in boot_log:
                            regs = q.call(
                                "human-monitor-command",
                                **{"command-line": "info registers"},
                            )
                            flags = re.search(r"RFL=([0-9a-fA-F]+)", regs)
                            if (
                                "HLT=1" in regs
                                and flags
                                and not int(flags[1], 16) & 0x200
                            ):
                                break
                        time.sleep(0.15)
                    else:
                        raise TimeoutError(
                            "Décision terminale OVMF ou diagnostic absente"
                        )
                    q.call("stop")
                    before = None
                    after = q.stats()
                if observe:
                    observe(q, "fin")
                q.events(False, extra_events)
                regs = q.call(
                    "human-monitor-command", **{"command-line": "info registers"}
                )
                Path("cpu-final.txt").write_text(regs)
                halted = "HLT=1" in regs
                report = snapshot(q)
                flags = re.search(r"RFL=([0-9a-fA-F]+)", regs)
                pc = re.search(r"RIP=([0-9a-fA-F]+)", regs)
                terminal_pc = int(pc[1], 16) if pc else None
                if report:
                    halted = bool(
                        halted
                        and flags
                        and not int(flags[1], 16) & 0x200
                        and terminal_pc is not None
                        and report["arret"] <= terminal_pc < report["arret"] + 8
                    )
            finally:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
        trace = Path("transactions.log").read_bytes()
        if boundary is not None and not boundary:
            raise ValueError("Témoin de trace firmware vide")
        during = trace[boundary:] if boundary is not None else b""
        bme = {}
        if boundary is not None:
            for model, bdf, value in re.findall(
                rb"pci_cfg_write (\S+) (\S+) @0x4 <- (0x[0-9a-f]+)\n", trace[:boundary]
            ):
                bme[(model + b" " + bdf).decode()] = int(value, 16)
        return dict(
            topologie=topology,
            image=digest(media),
            magasin=digest(variables),
            initial=start,
            entree=before,
            fin=after,
            rapport=report,
            octets_avant=boundary,
            octets_trace=len(trace),
            evenements_diagnostic=during.decode().splitlines(),
            commandes_pci_entree=bme,
            octets_avant_sortie=handoff,
            rip_arret=rip,
            halt=halted,
            rip_final=terminal_pc,
            disque_intact=internal_hash == digest(Path("interne.raw")),
        )
    finally:
        os.chdir(origin)
