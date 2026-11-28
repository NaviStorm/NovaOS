"""Relecture FAT Linux depuis un Alpine Live en RAM, sans disque interne ni réseau."""

import argparse
import json
import os
from pathlib import Path
import re
import select
import subprocess
import sys
import time
import zipfile

from support import OS, empreinte, require

sys.path.insert(0, str(OS / "tests/ethernet_v1"))
from port_qemu import qualification


def until(proc, pattern, limit):
    data = bytearray()
    end = time.monotonic() + limit
    while not re.search(pattern, data):
        require(
            time.monotonic() < end and len(data) < 2 << 20,
            "Console Linux hors délai ou hors bornes",
        )
        if select.select([proc.stdout], [], [], 0.25)[0]:
            chunk = os.read(proc.stdout.fileno(), 65536)
            require(chunk, "Fin inattendue de la console Linux")
            data.extend(chunk)
    return bytes(data)


def run(iso, image, archive, work):
    work.mkdir(mode=0o700)
    require(iso.is_file() and image.is_file(), "Supports virtuels ordinaires attendus")
    subprocess.run(
        [
            "xorriso",
            "-osirrox",
            "on",
            "-indev",
            str(iso),
            "-extract",
            "/boot/vmlinuz-lts",
            str(work / "kernel"),
            "-extract",
            "/boot/initramfs-lts",
            str(work / "initrd"),
        ],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    args = [
        "qemu-system-aarch64",
        "-machine",
        "virt",
        "-accel",
        "hvf",
        "-cpu",
        "host",
        "-m",
        "1G",
        "-smp",
        "2",
        "-nodefaults",
        "-display",
        "none",
        "-monitor",
        "none",
        "-serial",
        "stdio",
        "-nic",
        "none",
        "-kernel",
        str(work / "kernel"),
        "-initrd",
        str(work / "initrd"),
        "-append",
        "console=ttyAMA0 modules=loop,squashfs,virtio_blk,xhci_pci,usb_storage,sd_mod quiet",
        "-drive",
        "if=none,id=iso,format=raw,readonly=on,file=" + str(iso),
        "-device",
        "virtio-blk-pci,drive=iso",
        "-device",
        "qemu-xhci,id=xhci",
        "-drive",
        "if=none,id=logs,format=raw,readonly=on,file=" + str(image),
        "-device",
        "usb-storage,bus=xhci.0,drive=logs",
        "-no-reboot",
    ]
    with (
        qualification(Path(__file__).resolve().parents[4], {}),
        (work / "qemu.log").open("wb") as err,
        subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err, bufsize=0
        ) as p,
    ):
        try:
            until(p, rb"login: ", 100)
            p.stdin.write(b"root\n")
            until(p, rb"[#] ", 20)
            p.stdin.write(b"stty -echo; echo PRET5N\n")
            until(p, rb"\r?\nPRET5N\r?\n", 10)
            p.stdin.write(
                b"modprobe xhci_pci; modprobe usb_storage; modprobe sd_mod; modprobe vfat; sleep 2; mdev -s; blkid; mkdir -p /mnt/5n; "
                b"mount -t vfat -o ro /dev/sda2 /mnt/5n && cd /mnt/5n && "
                b"for i in 1 2 3 4 5 6 7 8; do for f in rapport.json JOURNAL.TXT VALIDES.SHA; "
                b"do sha256sum BOOT000$i/$f; done; done; echo FIN5N\n"
            )
            output = until(p, rb"\r?\nFIN5N\r?\n", 30)
            (work / "lecture-console.txt").write_bytes(output)
        finally:
            p.terminate()
            p.wait(timeout=10)
    matches = re.findall(rb"([0-9a-f]{64})  (BOOT000[1-8]/[\w.]+)", output)
    found = {n.decode(): h.decode() for h, n in matches}
    with zipfile.ZipFile(archive) as ref:
        require(
            len(matches) == 24
            and found == {n: empreinte(ref.read(n)) for n in ref.namelist()},
            "Lecture FAT Linux incomplète",
        )
    return dict(
        lecteur="Linux ARM64 Alpine, VFS vfat", lecture_seule=True, fichiers=found
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("iso", "image", "rapports", "travail"):
        p.add_argument(name, type=Path)
    a = p.parse_args()
    print(
        json.dumps(
            run(
                a.iso.resolve(),
                a.image.resolve(),
                a.rapports.resolve(),
                a.travail.resolve(),
            ),
            indent=2,
        )
    )
