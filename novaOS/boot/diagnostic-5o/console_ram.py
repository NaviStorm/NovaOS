"""Sonde Q35 en lecture seule du GOP RAM réservé par le module OVMF 5J."""

from hashlib import sha256
from pathlib import Path
import re
import struct
from demarrage import ram, require

# Symboles ELF mQemuRamfbMode et mQemuRamfbModeInfo du module identifié.
TEXTE, FIN_TEXTE, INFOS, MODE = 0x240, 0x1580, 0x1580, 0x15F0
MODULE = "f73aebffc4c217ebd4fecc8d521e267ca1d824a90dc01366d8ba3e049624b363"


def localiser(data, reference):
    require(sha256(reference).hexdigest() == MODULE, "Identité du pilote GOP OVMF")
    code = reference[TEXTE:FIN_TEXTE]
    bases = [
        m.start() - TEXTE
        for m in re.finditer(re.escape(code), data)
        if (m.start() - TEXTE) % 4096 == 0
        and data[m.start() - TEXTE : m.start() - TEXTE + 2] == b"MZ"
    ]
    require(len(bases) == 1, "Code GOP chargé absent ou ambigu")
    return bases[0]


def decrire(q, base, reference):
    module = ram(q, base, 0x1900)
    require(
        module[TEXTE:FIN_TEXTE] == reference[TEXTE:FIN_TEXTE], "Code du GOP modifié"
    )
    maximum, mode, info, sz, addr, n = struct.unpack_from("<II4Q", module, MODE)
    require(
        maximum == 3
        and mode < maximum
        and info == base + INFOS + 36 * mode
        and sz == 36,
        "Mode GOP non attribuable",
    )
    revision, w, h, fmt, *masks, stride = struct.unpack_from("<9I", module, info - base)
    require(
        (w, h) == [(640, 480), (800, 600), (1024, 768)][mode]
        and fmt == 1
        and revision == 0
        and not any(masks)
        and stride == w,
        "Format non reconnu",
    )
    require(
        addr % 4096 == 0
        and 1 << 20 <= addr
        and addr + n <= 256 << 20
        and n == 1024 * 768 * 4,
        "Allocation réservée du pilote GOP hors profil",
    )
    return (addr, n, w, h, stride, fmt)


def projeter(q, surface, name):
    """Projection PPM de la surface RAM avant ou après exécution du premier EFI."""
    base, n, w, h, stride, _ = surface
    data = ram(q, base, n)
    rgb = bytearray(w * h * 3)
    for y in range(h):
        line = data[4 * y * stride : 4 * (y * stride + w)]
        pos = 3 * y * w
        for channel, source in enumerate((2, 1, 0)):
            rgb[pos + channel : pos + 3 * w : 3] = line[source::4]
    Path(name).write_bytes(f"P6\n{w} {h}\n255\n".encode() + rgb)
    return w, h, rgb
