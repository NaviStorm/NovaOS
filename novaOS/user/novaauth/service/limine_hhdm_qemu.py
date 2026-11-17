"""Installer le chargeur HHDM corrigé dans l'export QEMU de lancement uniquement."""
import hashlib
from pathlib import Path
import struct

OFFICIAL_CD = "c25328b0e58f188d6b1a588827596d1c19a9e1d0d5e9dbc16a3aab238deb4aa4"
OFFICIAL_ARM = "bdf0c089d40ea7de9879a43c2a3ea648f1ea0b2eb297e8b1a750511db872b2fb"
LAB_ARM = "7b9cae50fc4649db9fb1ed48102c2b90d5df0ae2ad1113149d1b7f8707349b7e"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def replace_arm(image, original, loader):
    # L'empreinte de l'ESP officielle fixe son unique entrée AArch64 et sa
    # géométrie FAT12. Les autres exécutables, le relais BootID et le noyau
    # proviennent du circuit habituel de création de l'ISO.
    if (digest(image), digest(original), digest(loader)) != (OFFICIAL_CD, OFFICIAL_ARM, LAB_ARM):
        raise ValueError("source Limine de qualification différente")
    sector, per_cluster, reserved, copies, entries, _, _, fat_sectors = struct.unpack_from(
        "<HBHBHHBH", image, 11)
    fat = reserved * sector
    fat_size = fat_sectors * sector
    data = fat + copies * fat_size + ((entries * 32 + sector - 1) // sector) * sector
    size = sector * per_cluster
    name = b"BOOTAA64EFI"
    if image.count(name) != 1 or copies != 2:
        raise ValueError("répertoire EFI inattendu")
    entry = image.index(name)
    first, old_size = struct.unpack_from("<HI", image, entry + 26)
    chain = []
    n = first
    while 2 <= n < 0xff0:
        if n in chain or data + (n - 1) * size > len(image):
            raise ValueError("chaîne FAT12 incorrecte")
        chain.append(n)
        n = (struct.unpack_from("<H", image, fat + n * 3 // 2)[0] >> (4 * (n % 2))) & 0xfff

    def cluster(k):
        start = data + (k - 2) * size
        return slice(start, start + size)

    if n < 0xff8 or old_size != len(original) or b"".join(image[cluster(k)] for k in chain)[:old_size] != original:
        raise ValueError("chargeur original absent de l'ESP")
    used = (len(loader) + size - 1) // size
    if not 0 < used <= len(chain):
        raise ValueError("chargeur de qualification incompatible avec l'ESP vérifiée")
    result = bytearray(image)
    for i, k in enumerate(chain):
        result[cluster(k)] = loader[i * size:(i + 1) * size].ljust(size, b"\0")
        link = chain[i + 1] if i + 1 < used else (0xfff if i + 1 == used else 0)
        for mirror in range(copies):
            p = fat + mirror * fat_size + k * 3 // 2
            shift = 4 * (k % 2)
            word = struct.unpack_from("<H", result, p)[0]
            struct.pack_into("<H", result, p, (word & ~(0xfff << shift)) | (link << shift))
    struct.pack_into("<I", result, entry + 28, len(loader))
    return bytes(result)


def install(export):
    dest = export / "novaOS/third_party/limine"
    arm = dest / "BOOTAA64.EFI"
    cd = dest / "limine-uefi-cd.bin"
    lab = Path(__file__).with_name("limine_qemu") / "BOOTAA64.EFI"
    loader = lab.read_bytes()
    image = replace_arm(cd.read_bytes(), arm.read_bytes(), loader)
    manifest = dest / "SHA256SUMS"
    sums = manifest.read_text()
    for old, new in ((OFFICIAL_CD, digest(image)), (OFFICIAL_ARM, LAB_ARM)):
        if sums.count(old) != 1:
            raise ValueError("manifeste Limine divergent")
        sums = sums.replace(old, new)
    cd.write_bytes(image)
    arm.write_bytes(loader)
    manifest.write_text(sums)
