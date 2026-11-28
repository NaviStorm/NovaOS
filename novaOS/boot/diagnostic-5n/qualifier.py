"""Campagne froide 5N. Toutes les destinations sont neuves, privées et régulières."""

import argparse
import dataclasses
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import zipfile
import zlib

from demarrage import STORES, boot
from journaux import SupportPrive, history, read_at
from support import (
    LOG,
    MIO,
    OS,
    S,
    SLOTS,
    build,
    gpt,
    require,
    verify,
)

sys.path.insert(0, str(OS / "tests/ethernet_v1"))
from port_qemu import qualification


def digest(p):
    import hashlib

    h = hashlib.sha256()
    with p.open("rb") as f:
        for b in iter(lambda: f.read(MIO), b""):
            h.update(b)
    return h.hexdigest()


def partition_absente(size):
    blocks = gpt(size // S)
    for lba, table in blocks.items():
        if len(table) == 16384:
            table[128:256] = bytes(128)
    for lba in (1, size // S - 1):
        header = blocks[lba]
        struct.pack_into("<I", header, 88, zlib.crc32(blocks[2]))
        struct.pack_into("<I", header, 16, 0)
        struct.pack_into("<I", header, 16, zlib.crc32(header[:92]))
    return blocks


def archive_reports(owner, output):
    # Une archive déterministe des octets relus, jamais de dump de disque modifié.
    with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED) as out:
        areas = verify(owner.f)
        for i, (state, _) in enumerate(history(owner.f, areas)):
            if state == "valide":
                for area, name in zip(
                    areas[i], ("rapport.json", "JOURNAL.TXT", "VALIDES.SHA")
                ):
                    info = zipfile.ZipInfo(
                        f"BOOT{i + 1:04d}/{name}", (1980, 1, 1, 0, 0, 0)
                    )
                    out.writestr(info, read_at(owner.f, area), zipfile.ZIP_DEFLATED)


def run(out, image, fw, evidence, horloge_attestee=False):
    out.mkdir(mode=0o700)
    evidence.mkdir()
    version = subprocess.check_output(
        ["qemu-system-x86_64", "--version"], text=True
    ).splitlines()[0]
    result = dict(schema=1, qemu=version, image=digest(image), magasins=STORES, cas={})

    def checkpoint():
        (evidence / "observations.json").write_text(
            json.dumps(result, ensure_ascii=False, indent=2) + "\n"
        )

    with qualification(Path(__file__).resolve().parents[4], {}):
        for profile, size, scenario in [
            ("standard", 68, "cycles"),
            ("grand-dd", 96, "fin"),
            ("grand-ajuste", 96, "nominal"),
            ("sans-logs", 68, "absent"),
            ("fat-alteree", 68, "fat"),
            ("ambigu", 68, "clone"),
            ("coupures", 68, "coupure"),
        ]:
            src = image
            if scenario == "nominal":
                src = out / "capacite-96.img"
                build(src, size)
            if scenario in ("absent", "fat"):
                src = out / (profile + "-fixture.img")
                with image.open("rb") as original, src.open("xb") as dst:
                    import shutil

                    shutil.copyfileobj(original, dst)
                    if scenario == "absent":
                        for k, v in partition_absente(size * MIO).items():
                            dst.seek(k * S)
                            dst.write(v)
                    else:
                        dst.seek((LOG + 32) * S)
                        dst.write(b"BAD!")
            owner = SupportPrive(out / profile, src, size * MIO)
            clone = (
                SupportPrive(out / (profile + "-double"), src, size * MIO)
                if scenario == "clone"
                else None
            )
            try:
                before_copy = digest(owner.path)
                with owner.reader() as f, src.open("rb") as reference:
                    while True:
                        block = reference.read(MIO)
                        if not block:
                            break
                        require(f.read(len(block)) == block, "Copie dd")
                result.setdefault("copies", {})[profile] = dict(
                    capacite_mio=size,
                    source=digest(src),
                    support_avant=before_copy,
                    prefixe_identique=True,
                )
                if scenario == "cycles":
                    modes = ["actif", "inactif"] * 4 + ["actif", "inactif"]
                elif scenario == "coupure":
                    modes = ["inactif"] * 4
                else:
                    modes = ["actif", "inactif"] if scenario == "fin" else ["actif"]
                bases = {}
                for i, mode in enumerate(modes):
                    name = f"{profile}-{i + 1:02}-{mode}"
                    with clone.reader() if clone else open(os.devnull, "rb") as extra:
                        d = extra if clone else None
                        if mode not in bases:
                            with owner.reader() as r:
                                bases[mode] = boot(
                                    out / ("cal-" + name), fw, mode, r, duplicate=d
                                )["base"]
                        oldhash = digest(owner.path)
                        fault = None
                        expected = ""
                        if scenario == "cycles" and i == SLOTS:
                            expected = "NOVALOGS saturée"
                        elif scenario == "cycles" and i == SLOTS + 1:
                            # Ce contre-exemple utilise une COPIE préparée avant le prochain lancement.
                            # Les huit rapports acquis sur le support original sont conservés.
                            archive_reports(owner, evidence / "rapports.zip")
                            damaged = out / "ancien-invalide.img"
                            with (
                                owner.path.open("rb") as orig,
                                damaged.open("xb") as dst,
                            ):
                                import shutil

                                shutil.copyfileobj(orig, dst)
                                areas = verify(orig)
                                dst.seek(areas[0][0][0])
                                dst.write(b"X")
                            owner.close()
                            owner = SupportPrive(
                                out / "ancien-invalide", damaged, size * MIO
                            )
                            oldhash = digest(owner.path)
                            expected = "Sceau ou ancien rapport altéré"
                        elif scenario == "fin":
                            expected = "GPT secondaire hors fin du support"
                        elif scenario in ("absent", "fat"):
                            expected = "Profil GPT/FAT32 ou contenu EFI altéré"
                        elif scenario == "clone":
                            expected = "Identité USB ambiguë"
                        elif scenario == "coupure":
                            if i == 0:
                                fault, expected = (
                                    1,
                                    "Interruption simulée après synchronisation",
                                )
                            elif i == 2:
                                fault, expected = (
                                    "sceau-dechire",
                                    "Coupure simulée pendant le sceau",
                                )
                            elif i == 3:
                                expected = "Sceau ou ancien rapport altéré"
                        item = owner.run(
                            out / name,
                            fw,
                            mode,
                            bases[mode],
                            duplicate=d,
                            fault=fault,
                            horloge_attestee=horloge_attestee,
                        )
                    require(
                        item.refus_export == expected,
                        "Export inattendu " + name + " " + item.refus_export,
                    )
                    afterhash = digest(owner.path)
                    if expected and not fault:
                        require(oldhash == afterhash, "Mutation lors d’un refus")
                    if scenario == "coupure" and i == 1:
                        require(
                            item.emplacement == "BOOT0002",
                            "Réemploi d’un emplacement interrompu",
                        )
                    if not expected:
                        require(bool(item.emplacement), "Publication absente")
                    result["cas"][name] = dict(
                        **dataclasses.asdict(item),
                        sha_avant=oldhash,
                        sha_apres=afterhash,
                    )
                    checkpoint()
            finally:
                owner.close()
                if clone:
                    clone.close()


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("image", "firmware", "travail", "preuves"):
        ap.add_argument(name, type=Path)
    ap.add_argument(
        "--horloge-utc-attestee",
        action="store_true",
        help="Horloge du banc vérifiée par l’opérateur",
    )
    a = ap.parse_args()
    run(
        a.travail.resolve(),
        a.image.resolve(),
        a.firmware.resolve(),
        a.preuves.resolve(),
        a.horloge_utc_attestee,
    )
