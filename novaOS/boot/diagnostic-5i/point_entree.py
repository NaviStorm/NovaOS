"""Contrôle du RVA et des octets exécutables sur la machine suspendue."""

import struct


def entry_witness(q, media, expected_rip):
    """Recouper le marqueur du débogueur avec l'AddressOfEntryPoint du PE réellement sur le FAT."""
    from pyfatfs.PyFatFS import PyFatFS
    from acpi_fige import ram_reader
    from temoin import require
    from hashlib import sha256

    with PyFatFS(str(media), read_only=True) as fs:
        image = fs.readbytes("EFI/BOOT/BOOTX64.EFI")
    pe = int.from_bytes(image[60:64], "little")
    opt = pe + 24
    require(
        image[:2] == b"MZ"
        and image[pe : pe + 4] == b"PE\0\0"
        and int.from_bytes(image[pe + 4 : pe + 6], "little") == 0x8664
        and image[opt : opt + 2] == b"\x0b\x02",
        "PE32+ x86_64",
    )
    rva = int.from_bytes(image[opt + 16 : opt + 20], "little")
    size = int.from_bytes(image[opt + 56 : opt + 60], "little")
    base = expected_rip - rva
    read = ram_reader(q)
    loaded = read(base, opt + 64)
    require(
        loaded[:64] == image[:64]
        and loaded[pe : opt + 24] == image[pe : opt + 24]
        and int.from_bytes(loaded[opt + 56 : opt + 60], "little") == size,
        "Image chargée ou RVA divergent",
    )
    n = int.from_bytes(image[pe + 6 : pe + 8], "little")
    sections = opt + int.from_bytes(image[pe + 20 : pe + 22], "little")
    matches = []
    for j in range(n):
        p = sections + j * 40
        virtual, raw, offset = struct.unpack_from("<III", image, p + 12)
        if virtual <= rva and rva + 12 <= virtual + raw:
            matches.append(offset + rva - virtual)
    require(len(matches) == 1, "Section du point d'entrée")
    witness = read(expected_rip, 12)
    require(
        image[matches[0] : matches[0] + 12] == witness and witness[0] == 0xFA,
        "Première instruction CLI",
    )
    return {
        "base": base,
        "rva": rva,
        "rip": expected_rip,
        "taille_image": size,
        "efi_sha256": sha256(image).hexdigest(),
        "code_entree": witness.hex(),
    }
