"""Archiver une campagne synthétique par liste fermée, sans mémoire générale ni clé privée."""

import argparse
import gzip
import json
from pathlib import Path
import struct
import zipfile
import zlib
from image5o import require

SCENARIOS = ("nominal", "absent", "epoque", "signature", "desaccord", "limites")
MODES = ("actif", "inactif")
PIECES = (
    "recu.bin",
    "surface.bin",
    "sortie.bin",
    "epoch.bin",
    "tables.bin",
    "unite.json",
    "compteurs-qmp.jsonl",
    "interruptions.json",
    "intervalle.log",
    "verdict-apic.json",
    "efi.log",
)


def png(ppm):
    a, shape, m, raw = ppm.split(b"\n", 3)
    w, h = map(int, shape.split())
    require(a == b"P6" and m == b"255" and len(raw) == w * h * 3, "Capture PPM")

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data))
        )

    lines = b"".join(b"\0" + raw[i : i + 3 * w] for i in range(0, len(raw), 3 * w))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(lines, 9))
        + chunk(b"IEND", b"")
    )


def archive(build, out):
    out.mkdir(mode=0o700)

    def save(n, data):
        with (out / n).open("xb") as f:
            f.write(data)

    with zipfile.ZipFile(
        out / "observations.zip", "x", compression=zipfile.ZIP_DEFLATED
    ) as z:

        def add(n, b):
            i = zipfile.ZipInfo(n, (2000, 1, 1, 0, 0, 0))
            i.external_attr = 0o100644 << 16
            z.writestr(i, b, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)

        for mode in MODES:
            g = build / ("gop-final-" + mode)
            add(mode + "/cal.json", (g / "cal.json").read_bytes())
            for cas in SCENARIOS:
                case = g / cas
                n = mode + "/" + cas + "/"
                add(n + "bilan.json", (g / (cas + ".json")).read_bytes())
                for piece in PIECES:
                    add(n + piece, (case / piece).read_bytes())
                # Conserver le préfixe intégral pour rattacher la première interruption.
                fin = json.loads((case / "interruptions.json").read_text())["fin"]
                add(n + "trace.log", (case / "trace.log").read_bytes()[:fin])
                for moment in ("avant", "apres"):
                    ppm = (case / (moment + ".ppm")).read_bytes()
                    add(n + moment + ".ppm", ppm)
                    if moment == "apres" and cas == "nominal":
                        save("verdict-" + mode + ".png", png(ppm))
        serie = build / "usb-a"
        usb = json.loads((serie / "bilan.json").read_text())
        extra = build / "usb-deux"
        if extra.is_dir():
            usb.update(json.loads((extra / "bilan.json").read_text()))
        for cas in usb:
            folder = (
                extra if extra.is_dir() and cas.startswith("ambigu-") else serie
            ) / cas
            if cas.startswith("ambigu-"):
                require(
                    "clone5n" in usb[cas]["compteurs_hlt"], "Compteurs du clone absents"
                )
            if not cas.startswith("hors-db"):
                for moment in ("avant", "apres"):
                    add(
                        "usb/" + cas + "/" + moment + ".ppm",
                        (folder / (moment + ".ppm")).read_bytes(),
                    )
            add("usb/" + cas + "/firmware.log", (folder / "firmware.log").read_bytes())
            add(
                "usb/" + cas + "/transactions.log",
                (folder / "transactions.log").read_bytes(),
            )
        for n in ("bilan.json", "lecture/firmware.log", "lecture/transactions.log"):
            add("anciens/" + n, (build / "anciens" / n).read_bytes())
        add("usb/bilan.json", (json.dumps(usb, indent=2) + "\n").encode())
        add(
            "garde-video-pci/barres.json",
            (build / "garde-pci/cal/barres.json").read_bytes(),
        )
        add("garde-video-pci/pci.json", (build / "garde-pci/cal/pci.json").read_bytes())
    save("capacites.json", (build / "capacites.json").read_bytes())
    save("variables.json", (build / "magasin/diff.json").read_bytes())
    save(
        "magasin-ovmf-test.fd.gz",
        gzip.compress((build / "magasin/actif.fd").read_bytes(), mtime=0),
    )
    save(
        "novaos-x86_64-68mio.img.gz",
        gzip.compress((build / "geometries/68-origine.img").read_bytes(), mtime=0),
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("build", type=Path)
    p.add_argument("sortie", type=Path)
    a = p.parse_args()
    archive(a.build, a.sortie)
