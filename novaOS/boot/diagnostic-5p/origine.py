"""Contre-lecture des handles EDK II x64 au premier RIP. Aucune invocation de protocole invité."""

import struct
from uuid import UUID
from observation import etendue, require, ram

IMAGE = UUID("5b1b31a1-9562-11d2-8e3f-00a0c969723b").bytes_le
DP = UUID("09576e91-6d3f-11d2-8e39-00a0c969723b").bytes_le


def u64(data, off):
    return struct.unpack_from("<Q", data, off)[0]


def chemin(read, addr):
    out = []
    for _ in range(32):
        kind, sub, size = struct.unpack("<BBH", read(addr, 4))
        require(4 <= size <= 1024, "Nœud DevicePath hors budget")
        b = read(addr, size)
        addr += size
        if (kind, sub, size) == (127, 255, 4):
            return out
        if (kind, sub, size) == (2, 1, 12):
            out.append(["acpi", *struct.unpack_from("<II", b, 4)])
        elif (kind, sub, size) == (1, 1, 6):
            out.append(["pci", b[5], b[4]])
        elif (kind, sub, size) == (3, 5, 6):
            out.append(["usb", b[4] + 1, b[5]])
        elif (kind, sub, size) == (3, 17, 5):
            out.append(["lun", b[4]])
        elif (kind, sub, size) == (4, 1, 42):
            out.append(
                [
                    "partition",
                    *struct.unpack_from("<IQQ", b, 4),
                    b[24:40].hex(),
                    b[40],
                    b[41],
                ]
            )
        elif (kind, sub) == (4, 4):
            require(b.endswith(b"\0\0"), "Fichier UTF16")
            out.append(["fichier", b[4:-2].decode("utf-16-le")])
        else:
            raise ValueError(f"Nœud non attribué {kind}/{sub}/{size}")
    raise ValueError("DevicePath sans fin")


def interface(read, handle, guid):
    h = read(handle, 40)
    require(h[:8] == b"hndl" + bytes(4), "Handle EDK II inconnu")
    head = handle + 24
    p = u64(h, 24)
    prev = head
    found = []
    seen = set()
    while p != head:
        require(p not in seen and len(seen) < 64, "Liste de protocoles ambiguë")
        seen.add(p)
        b = read(p - 8, 64)
        require(
            b[:8] == b"pifc" + bytes(4) and u64(b, 16) == prev and u64(b, 24) == handle,
            "Protocole hors titulaire",
        )
        pr = read(u64(b, 48), 40)
        require(pr[:8] == b"prte" + bytes(4), "Entrée protocole")
        if pr[24:40] == guid:
            found.append(u64(b, 56))
        prev, p = p, u64(b, 8)
    require(prev == u64(h, 32) and len(found) == 1, "Protocole absent ou dupliqué")
    return found[0]


def provenance(q, dbg, spans, base, image_size):
    def read(a, n):
        return ram(q, etendue(a, n, 1, spans), n)

    handle = struct.unpack_from("<17Q", bytes.fromhex(dbg.command("g")))[2]
    img = read(interface(read, handle, IMAGE), 88)
    require(
        u64(img, 64) == base and u64(img, 72) == image_size,
        "LoadedImage différent de la mémoire mesurée",
    )
    device = chemin(read, interface(read, u64(img, 24), DP))
    fichier = chemin(read, u64(img, 32))
    require(
        [n[1].upper() for n in fichier if n[0] == "fichier"]
        == ["\\EFI\\BOOT\\BOOTX64.EFI"],
        "Nom du premier EFI",
    )
    return dict(peripherique=device, fichier=fichier)
