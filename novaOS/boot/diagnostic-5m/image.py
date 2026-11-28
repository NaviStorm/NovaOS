"""Restitution des sections PE32+ du diagnostic après les relocations DIR64 standard."""

from hashlib import sha256
import struct


def sections_chargees(pe, base):
    p = struct.unpack_from("<I", pe, 60)[0]
    opt = p + 24
    n = struct.unpack_from("<H", pe, p + 6)[0]
    table = opt + struct.unpack_from("<H", pe, p + 20)[0]
    taille = struct.unpack_from("<I", pe, opt + 56)[0]
    if not 0 < taille <= 8 << 20 or not 1 <= n <= 16:
        raise ValueError("Géométrie PE")
    image = bytearray(taille)
    sections = {}
    for i in range(n):
        s = table + 40 * i
        nom = pe[s : s + 8].rstrip(b"\0").decode("ascii")
        vs, va, sz, off = struct.unpack_from("<IIII", pe, s + 8)
        if nom in sections or not 0 < vs <= taille - va or off + sz > len(pe):
            raise ValueError("Section PE")
        if any(
            va < start + size and start < va + vs for start, size in sections.values()
        ):
            raise ValueError("Chevauchement PE")
        sections[nom] = va, vs
        count = min(vs, sz)
        image[va : va + count] = pe[off : off + count]
    debut, longueur = struct.unpack_from("<II", pe, opt + 112 + 8 * 5)
    if sections.get(".reloc") != (debut, longueur):
        raise ValueError("Table de relocations")
    pos = debut
    fin = debut + longueur
    delta = base - struct.unpack_from("<Q", pe, opt + 24)[0]
    while pos < fin:
        page, sz = struct.unpack_from("<II", image, pos)
        if not 8 <= sz <= fin - pos or sz % 2:
            raise ValueError("Bloc de relocations")
        for (e,) in struct.iter_unpack("<H", image[pos + 8 : pos + sz]):
            if e >> 12 == 0:
                continue
            adr = page + (e & 4095)
            if e >> 12 != 10 or not any(
                a <= adr <= a + n - 8 for a, n in sections.values()
            ):
                raise ValueError("Type de relocation")
            val = struct.unpack_from("<Q", image, adr)[0]
            struct.pack_into("<Q", image, adr, (val + delta) & ((1 << 64) - 1))
        pos += sz
    return {
        nom: (adr, sz, sha256(image[adr : adr + sz]).hexdigest())
        for nom, (adr, sz) in sections.items()
    }


def check(condition, raison):
    if not condition:
        raise ValueError(raison)


def pe_info(b):
    p = struct.unpack_from("<I", b, 60)[0]
    check(b[:2] == b"MZ" and b[p : p + 6] == b"PE\0\0\x64\x86", "PE x86_64")
    opt = p + 24
    check(
        b[opt : opt + 2] == b"\x0b\x02" and b[opt + 144 : opt + 152] == bytes(8),
        "PE32+ de laboratoire sans certificat",
    )
    n = struct.unpack_from("<H", b, p + 6)[0]
    sec = opt + struct.unpack_from("<H", b, p + 20)[0]
    hdr = struct.unpack_from("<I", b, opt + 60)[0]
    end = hdr
    code = None
    for i in range(n):
        a = sec + 40 * i
        rva, sz, off = struct.unpack_from("<III", b, a + 12)
        if not sz:
            continue
        check(off == end and off + sz <= len(b), "Sections non contiguës")
        if b[a : a + 8].rstrip(b"\0") == b".text":
            code = (rva, off, sz)
        end += sz
    check(code and end == len(b), "Disposition PE inconnue")
    h = sha256(b[: opt + 64] + b[opt + 68 : opt + 144] + b[opt + 152 :])
    rva, off, sz = code
    section = b[off : off + sz]
    check(section.count(b"\xf4\xeb\xfd") == 1, "Arrêt terminal ambigu")
    return dict(
        authenticode=h.hexdigest(),
        sha256=sha256(b).hexdigest(),
        entree=struct.unpack_from("<I", b, opt + 16)[0],
        taille=struct.unpack_from("<I", b, opt + 56)[0],
        texte=[rva, off, sz],
        hlt=rva + section.index(b"\xf4\xeb\xfd"),
    )
