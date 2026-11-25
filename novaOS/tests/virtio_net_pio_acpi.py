"""Vérifie les ressources AML produites par chaque modèle avant la première instruction."""
import argparse
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile

from ovmf_reseau_q35 import arguments
from virtio_net_confinement import ROOT, digest, dump, hmp, qmp
from virtio_net_pio_q35 import qt_command
from virtio_net_qemu import qualification


def lire(qt, taille):
    data = bytearray()
    while len(data) < taille:
        n = min(256, taille - len(data))
        qt.write(b"inb 0x511\n" * n)
        for _ in range(n):
            parts = qt.readline().split()
            if len(parts) != 2 or parts[0] != b"OK":
                raise ValueError("Lecture du port fw_cfg incomplète")
            data.append(int(parts[1], 16))
    return data


def aml(qt, out, dma):
    qt_command(qt, "outw 0x510 0x19")
    n = int.from_bytes(lire(qt, 4), "big")
    if not 0 < n <= 256:
        raise ValueError("Répertoire fw_cfg hors budget")
    tables = []
    for _ in range(n):
        size, selector, unused, name = struct.unpack(">IHH56s", lire(qt, 64))
        if name.rstrip(b"\0") == b"etc/acpi/tables":
            tables.append((selector, size))
    if len(tables) != 1 or not 36 <= tables[0][1] <= 1 << 20:
        raise ValueError("Tables ACPI absentes ou ambiguës")
    selector, size = tables[0]
    qt_command(qt, f"outw 0x510 {selector:#x}")
    content = lire(qt, size)
    addr = content.find(b"DSDT")
    if addr < 0:
        raise ValueError("DSDT absente")
    length = struct.unpack_from("<I", content, addr + 4)[0]
    if not 36 <= length <= len(content) - addr:
        raise ValueError("DSDT tronquée")
    dsdt = content[addr:addr+length]
    ident = dsdt.find(b"QEMU0002\0")
    descriptor = b"\x47\x01\x10\x05\x10\x05\x01" + bytes((12 if dma else 2,))
    if ident < 0 or dsdt.count(b"QEMU0002\0") != 1 or dsdt[ident:ident+80].count(descriptor) != 1:
        raise ValueError("Le périphérique FWCF ne publie pas le descripteur IO attendu")
    (out / "DSDT.aml").write_bytes(dsdt)
    return {"taille_crs_ports": descriptor[-1], "sha256_dsdt": digest(out / "DSDT.aml")}


def verifier(firmware, entree, out, corrige, dma):
    out.mkdir()
    args = arguments(firmware / "OVMF_CODE.fd", entree, out, False, acpi_pio=corrige)
    args += ["-global", "fw_cfg_io.dma_enabled=" + ("on" if dma else "off")]
    report = {}
    dump(out / "arguments.json", args)
    try:
        with qualification(ROOT.parent.parent, report), tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp:
            with (out / "qemu.trace").open("xb") as log, subprocess.Popen(args,cwd=tmp,stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,stderr=log,bufsize=0) as vm:
                try:
                    qmp(vm,"qmp_capabilities")
                    if qmp(vm,"query-status")["running"] or qmp(vm,"qom-get",path="/machine/fw_cfg",property="dma_enabled") is not dma:
                        raise ValueError("État initial du modèle incorrect")
                    mtree = hmp(vm,"info mtree")
                    (out / "reset-mtree.txt").write_text(mtree)
                    if ("fwcfg.dma" in mtree) != dma:
                        raise ValueError("Plage DMA et propriété QOM incohérentes")
                    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as s:
                        s.connect(str(Path(tmp)/"qtest.sock"))
                        with s.makefile("rwb",buffering=0) as qt:
                            report.update(aml(qt,out,dma))
                    qmp(vm,"quit")
                    if vm.wait(timeout=5):
                        raise ValueError("Arrêt QEMU anormal")
                    report["reussi"]=True
                finally:
                    if vm.poll() is None:
                        vm.kill()
                    vm.wait()
    finally:
        dump(out/"campagne.json",report)


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    for nom in ("firmware","entree","sortie"):
        parser.add_argument(nom,type=Path)
    parser.add_argument("--corrige",action="store_true")
    parser.add_argument("--dma",action="store_true")
    a=parser.parse_args()
    verifier(a.firmware.resolve(),a.entree.resolve(),a.sortie.resolve(),a.corrige,a.dma)
