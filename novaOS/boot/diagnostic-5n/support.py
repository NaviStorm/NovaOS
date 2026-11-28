"""Image GPT x86_64 à secteurs de 512 octets. Les sorties sont des fichiers neufs."""

import argparse
import gzip
import hashlib
from functools import lru_cache
import json
from pathlib import Path
import struct
import sys
import tarfile
import uuid
import zlib

OS = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(OS / "scripts"))
from esp_installation import nom_long, secteur_entree

S = 512
MIO = 1 << 20
MIN_MIO = 68
PART = 33 * MIO // S
ESP = MIO // S
LOG = ESP + PART
RES = 32
FAT = ((PART - RES + 2) * 4 + S + 7) // (S + 8)
DATA = RES + 2 * FAT
CLUSTERS = PART - DATA
SLOTS = 8
PE_SHA = "9442959bb9dcfcccba17c8f988684af6615f38e45325fd114c989973ce39a4d7"
ARCHIVE = OS / "docs/validation/diagnostic-uefi-5m/supports.tar.gz"
TYPES = ("c12a7328-f81f-11d2-ba4b-00a0c93ec93b", "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7")


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def empreinte(data):
    return hashlib.sha256(data).hexdigest()


def public_member(name, sha):
    with tarfile.open(ARCHIVE, "r:gz") as archive:
        m = archive.getmember(name)
        require(m.isfile() and m.size <= 1 << 20, "Membre 5M hors profil")
        data = archive.extractfile(m).read()
    require(empreinte(data) == sha, "Empreinte 5M")
    return data


@lru_cache(maxsize=1)
def notices():
    archive = OS / "docs/validation/diagnostic-uefi-5e/laboratoire.tar.gz"
    require(
        empreinte(archive.read_bytes())
        == "9c41a5b91b689391efc48c82d2e143c9c351c2242682bfb281bd93025a0a44c5",
        "Archive de licences 5E",
    )
    with tarfile.open(archive, "r:gz") as a:
        return b"".join(
            m.name.encode() + b"\n" + a.extractfile(m).read() + b"\n"
            for m in a.getmembers()
            if m.isfile() and m.name.startswith("licences/")
        )


def guid(name):
    return uuid.uuid5(uuid.NAMESPACE_URL, "urn:novaos:5n:" + name).bytes_le


def gpt(total):
    require(MIN_MIO * MIO // S <= total < 2**32, "Capacité GPT hors profil")
    table = bytearray(16384)
    for i, (name, start) in enumerate((("NOVAOS", ESP), ("NOVALOGS", LOG))):
        struct.pack_into(
            "<16s16sQQQ72s",
            table,
            128 * i,
            uuid.UUID(TYPES[i]).bytes_le,
            guid(name),
            start,
            start + PART - 1,
            0,
            name.encode("utf-16-le"),
        )
    mbr = bytearray(S)
    struct.pack_into(
        "<B3sB3sII", mbr, 446, 0, b"\0\x02\0", 0xEE, b"\xff" * 3, 1, total - 1
    )
    mbr[510:] = b"\x55\xaa"
    blocks = {0: mbr, 2: table, total - 33: table}
    for here, other, entries in ((1, total - 1, 2), (total - 1, 1, total - 33)):
        head = bytearray(S)
        struct.pack_into(
            "<8sIIIIQQQQ16sQIII",
            head,
            0,
            b"EFI PART",
            65536,
            92,
            0,
            0,
            here,
            other,
            34,
            total - 34,
            guid("disque"),
            entries,
            128,
            128,
            zlib.crc32(table),
        )
        struct.pack_into("<I", head, 16, zlib.crc32(head[:92]))
        blocks[here] = head
    return blocks


def volume(start, name, pe=None, *, fichiers_racine=()):
    blocks = {}
    table = [0x0FFFFFF8, 0x0FFFFFFF]

    def allocate(count):
        first = len(table)
        table.extend(range(first + 1, first + count + 1))
        table[-1] = 0x0FFFFFFF
        return first

    def address(cluster):
        return start + DATA + cluster - 2

    def directory(name, cluster):
        return secteur_entree(name, cluster, dossier=True)

    def links(cluster, parent):
        return directory(b".          ", cluster) + directory(b"..         ", parent)

    label = bytearray(32)
    label[:11] = name.encode().ljust(11)
    label[11] = 8
    root = bytes(label)
    allocate(1)
    areas = []
    if pe:
        efi, boot, first = allocate(1), allocate(1), allocate((len(pe) + S - 1) // S)
        root += directory(b"EFI        ", efi)
        blocks[address(efi)] = links(efi, 0) + directory(b"BOOT       ", boot)
        blocks[address(boot)] = links(boot, efi) + secteur_entree(
            b"BOOTX64 EFI", first, len(pe)
        )
        blocks[address(first)] = pe
        legal = notices()
        first = allocate((len(legal) + S - 1) // S)
        root += secteur_entree(b"LICENCESTXT", first, len(legal))
        blocks[address(first)] = legal
        for court, long, contenu in fichiers_racine:
            require(len(court) == 11 and contenu, "Fichier FAT invalide")
            n = (len(contenu) + S - 1) // S
            require(len(table) + n <= CLUSTERS + 2, "ESP pleine")
            first = allocate(n)
            if long:
                require(len(long.encode("utf-16-le")) <= 24, "Nom long hors profil")
                root += nom_long(long, court)
            root += secteur_entree(court, first, len(contenu))
            require(len(root) <= S, "Répertoire racine plein")
            blocks[address(first)] = contenu
    else:
        for slot in range(1, SLOTS + 1):
            sub = allocate(1)
            root += directory(f"BOOT{slot:04d}   ".encode(), sub)
            entries = links(sub, 0)
            parts = []
            for filename, count in (
                (b"RAPPOR~1JSN", 4),
                (b"JOURNAL TXT", 2),
                (b"VALIDES SHA", 1),
            ):
                if filename == b"RAPPOR~1JSN":
                    entries += nom_long("rapport.json", filename)
                first = allocate(count)
                entries += secteur_entree(filename, first, count * S)
                parts.append((address(first) * S, count * S))
            areas.append(parts)
            blocks[address(sub)] = entries
    blocks[address(2)] = root
    b = bytearray(S)
    b[:11] = b"\xeb\x58\x90NOVAOS  "
    struct.pack_into(
        "<HBHBHHBHHHII", b, 11, S, 1, RES, 2, 0, 0, 0xF8, 0, 63, 255, start, PART
    )
    struct.pack_into("<IHHIHH", b, 36, FAT, 0, 0, 2, 1, 6)
    b[64], b[66] = 0x80, 0x29
    struct.pack_into("<I", b, 67, zlib.crc32(label[:11]))
    b[71:90] = bytes(label[:11]) + b"FAT32   "
    b[510:] = b"\x55\xaa"
    info = bytearray(S)
    info[:4], info[484:488], info[508:] = b"RRaA", b"rrAa", b"\0\0\x55\xaa"
    require(CLUSTERS >= 65525 and (CLUSTERS + 2) * 4 <= FAT * S, "Géométrie FAT32")
    struct.pack_into("<II", info, 488, CLUSTERS - len(table) + 2, len(table))
    blocks[start] = blocks[start + 6] = b
    blocks[start + 1] = blocks[start + 7] = info
    blocks[start + RES] = blocks[start + RES + FAT] = struct.pack(
        f"<{len(table)}I", *table
    )
    return blocks, areas


def layout(total):
    blocks = gpt(total)
    blocks.update(volume(ESP, "NOVAOS", public_member("valide.efi", PE_SHA))[0])
    logs, areas = volume(LOG, "NOVALOGS")
    blocks.update(logs)
    return blocks, areas


def build(path, mio=MIN_MIO, *, secteurs=None):
    total = mio * MIO // S if secteurs is None else secteurs
    blocks, _ = layout(total)
    with path.open("xb") as f:
        f.truncate(total * S)
        for lba, data in sorted(blocks.items()):
            f.seek(lba * S)
            f.write(data)


def verify(f, allow_tail=False):
    """Comparer tout le profil immuable, y compris les zones libres, avant d'exposer les étendues."""
    size = f.seek(0, 2)
    f.seek(S + 32)
    field = f.read(8)
    require(len(field) == 8, "GPT tronquée")
    total = struct.unpack("<Q", field)[0] + 1
    require(
        size % S == 0 and (size == total * S or allow_tail and size > total * S),
        "GPT secondaire hors fin du support",
    )
    blocks, areas = layout(total)
    mutable = sorted((pos, length) for slot in areas for pos, length in slot)
    cursor = 0

    def zeros(end):
        nonlocal cursor
        while cursor < end:
            length = min(MIO, end - cursor)
            require(f.read(length) == bytes(length), "Zone réservée ou libre altérée")
            cursor += length

    f.seek(0)
    for pos, length, expected in sorted(
        [(k * S, len(v), v) for k, v in blocks.items()]
        + [(p, n, None) for p, n in mutable]
    ):
        require(pos >= cursor, "Chevauchement de plages")
        zeros(pos)
        if expected is None:
            f.seek(length, 1)
        else:
            require(
                f.read(length) == expected, "Profil GPT/FAT32 ou contenu EFI altéré"
            )
        cursor += length
    zeros(size)
    return areas


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("action", choices=["creer", "verifier", "compresser"])
    ap.add_argument("image", type=Path)
    size = ap.add_mutually_exclusive_group()
    size.add_argument("--mio", type=int, default=MIN_MIO)
    size.add_argument(
        "--secteurs",
        type=int,
        help="Capacité exacte en secteurs logiques de 512 octets",
    )
    ap.add_argument("--sortie", type=Path)
    a = ap.parse_args()
    if a.action == "creer":
        build(a.image, a.mio, secteurs=a.secteurs)
    elif a.action == "verifier":
        with a.image.open("rb") as f:
            verify(f)
        print(
            json.dumps(
                {
                    "partitions": ["NOVAOS", "NOVALOGS"],
                    "clusters_par_FAT32": CLUSTERS,
                    "rapports": SLOTS,
                }
            )
        )
    else:
        import shutil

        require(a.sortie is not None, "Sortie gzip attendue")
        with a.image.open("rb") as src, a.sortie.open("xb") as out:
            with gzip.GzipFile(
                filename="", fileobj=out, mode="wb", mtime=0, compresslevel=9
            ) as gz:
                shutil.copyfileobj(src, gz)


if __name__ == "__main__":
    main()
