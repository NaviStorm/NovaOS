"""Ajouter le relais AArch64 à une copie de l’image FAT12 officielle vérifiée."""
from pathlib import Path
import struct
import sys


def ajouter(image, relais):
    # Géométrie de l’image Limine contrôlée par SHA256SUMS dans iso.sh.
    sector, spc, reserved, copies, entries, total, _, fat_sectors = struct.unpack_from(
        "<HBHBHHBH", image, 11)
    if (sector, spc, copies) != (512, 2, 2) or total * sector != len(image):
        raise ValueError("géométrie FAT12 inattendue")
    fat_size = sector * fat_sectors
    fat_start = reserved * sector
    root = fat_start + copies * fat_size
    data = root + ((entries * 32 + sector - 1) // sector) * sector
    size = sector * spc
    fat = bytearray(image[fat_start:fat_start + fat_size])

    def cluster(n):
        if not 2 <= n < 2 + (len(image) - data) // size:
            raise ValueError("cluster hors de l’image")
        return data + (n - 2) * size

    def find(start, length, name):
        for pos in range(start, start + length, 32):
            if image[pos:pos + len(name)] == name:
                return pos
        raise ValueError(f"entrée EFI absente {name!r}")

    directory = root
    length = entries * 32
    for name in (b"EFI        ", b"BOOT       "):
        pos = find(directory, length, name)
        directory = cluster(struct.unpack_from("<H", image, pos + 26)[0])
        length = size
    original = find(directory, size, b"BOOTAA64EFI")
    new = find(directory, size, b"\x00")
    image[new:new + 32] = image[original:original + 32]
    image[original:original + 11] = b"LIMINE  EFI"

    free = []
    for n in range(2, 2 + (len(image) - data) // size):
        shift = 4 * (n % 2)
        value = struct.unpack_from("<H", fat, n * 3 // 2)[0]
        if ((value >> shift) & 0xfff) == 0:
            free.append(n)
        if len(free) * size >= len(relais):
            break
    if not relais or len(free) * size < len(relais):
        raise ValueError("relais vide ou image EFI pleine")
    for i, (n, successor) in enumerate(zip(free, free[1:] + [0xfff])):
        off = n * 3 // 2
        shift = 4 * (n % 2)
        word = struct.unpack_from("<H", fat, off)[0]
        struct.pack_into("<H", fat, off, (word & ~(0xfff << shift)) | (successor << shift))
        at = cluster(n)
        image[at:at + size] = relais[i * size:(i + 1) * size].ljust(size, b"\x00")
    for i in range(copies):
        at = fat_start + i * fat_size
        image[at:at + fat_size] = fat
    struct.pack_into("<HI", image, new + 26, free[0], len(relais))


if __name__ == "__main__":
    source, relay, output = map(Path, sys.argv[1:])
    content = bytearray(source.read_bytes())
    ajouter(content, relay.read_bytes())
    output.write_bytes(content)
