"""Cliché UEFI/ACPI borné du Q35 arrêté, sans exécution de service firmware."""

import re
import struct
import uuid
import zlib
from pathlib import Path
from sonde import cpu_flatview, hmp


def ram_reader(q):
    view = cpu_flatview(hmp(q, "info mtree -f"))
    ranges = [
        (int(a, 16), int(b, 16))
        for a, b in re.findall(
            r"(?m)^\s*([0-9a-f]+)-([0-9a-f]+) \(prio -?\d+, ram\): pc.ram(?: |$)", view
        )
    ]
    budget = 1024 * 1024

    def read(addr, size):
        nonlocal budget
        if (
            size <= 0
            or size > 128 * 1024
            or size > budget
            or addr <= 0
            or not any(a <= addr and addr + size - 1 <= b for a, b in ranges)
        ):
            raise ValueError("Adresse hors de la RAM prouvée ou budget épuisé")
        budget -= size
        p = Path("fragment-ram.bin")
        if p.exists():
            raise ValueError("Fragment préexistant")
        q.call("pmemsave", val=addr, size=size, filename=p.name)
        data = p.read_bytes()
        p.unlink()
        if len(data) != size:
            raise ValueError("Lecture tronquée")
        return data

    return read


def tables(q):
    if q.call("query-status")["running"]:
        raise ValueError("CPU actif")
    read = ram_reader(q)
    reg = hmp(q, "info registers")
    rdx = re.search(r"\bRDX=([0-9a-fA-F]+)", reg)
    if not rdx:
        raise ValueError("Table système x64 absente")
    phys = int(rdx[1], 16)
    # OVMF x86_64 de laboratoire. Contrôler l'identité du pointeur virtuel d'entrée.
    translation = hmp(q, f"gva2gpa {phys:#x}").strip()
    if (
        not re.fullmatch(r"(?:gpa: )?0x[0-9a-fA-F]+", translation)
        or int(translation.split()[-1], 16) != phys
    ):
        raise ValueError("Adresse de table système non identitaire : " + translation)
    system = read(phys, 120)
    n, crc = struct.unpack_from("<II", system, 12)
    if (
        system[:8] != b"IBI SYST"
        or n != 120
        or zlib.crc32(system[:16] + bytes(4) + system[20:]) != crc
    ):
        raise ValueError("En-tête de table système EFI")
    count, configs = struct.unpack_from("<QQ", system, 104)
    if not 1 <= count <= 1024:
        raise ValueError("Nombre de GUID EFI")
    entries = read(configs, count * 24)
    guid = uuid.UUID("8868e871-e4f1-11d3-bc22-0080c73c8881").bytes_le
    hits = [
        struct.unpack_from("<Q", entries, i + 16)[0]
        for i in range(0, len(entries), 24)
        if entries[i : i + 16] == guid
    ]
    if len(hits) != 1:
        raise ValueError("ACPI 2 absent ou multiple")
    start = read(hits[0], 36)
    length = struct.unpack_from("<I", start, 20)[0]
    if not 36 <= length <= 256:
        raise ValueError("Longueur RSDP")
    rsdp = read(hits[0], length)
    if (
        rsdp[:8] != b"RSD PTR "
        or rsdp[15] < 2
        or sum(rsdp[:20]) % 256
        or sum(rsdp) % 256
    ):
        raise ValueError("RSDP")
    spans = [(hits[0], hits[0] + length)]

    def table(addr):
        header = read(addr, 36)
        size = struct.unpack_from("<I", header, 4)[0]
        if size < 36 or any(lo < addr + size and addr < hi for lo, hi in spans):
            raise ValueError("Table ACPI tronquée ou recouvrante")
        spans.append((addr, addr + size))
        blob = read(addr, size)
        if blob[:36] != header or sum(blob) % 256:
            raise ValueError("Intégrité ACPI")
        return blob

    root = table(struct.unpack_from("<Q", rsdp, 24)[0])
    if (
        root[:4] != b"XSDT"
        or len(root[36:]) % 8
        or not 1 <= (len(root) - 36) // 8 <= 128
    ):
        raise ValueError("XSDT")
    out = {}
    for (ptr,) in struct.iter_unpack("<Q", root[36:]):
        blob = table(ptr)
        if blob[:4] not in (b"MCFG", b"DMAR", b"FACP"):
            continue
        key = blob[:4].decode()
        if key in out:
            raise ValueError("Table multiple " + key)
        out[key] = blob
    if "FACP" not in out or "MCFG" not in out:
        raise ValueError("FACP/MCFG manquante")
    return out
