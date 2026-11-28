"""Construire le support x86_64 sur fichier ordinaire, avec les deux FAT32 historiques."""

import argparse
from hashlib import blake2b, sha256
import mmap
import os
import stat
from pathlib import Path
import shutil
import struct
import subprocess
import sys

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5n"))
from support import ESP, LOG, MIN_MIO, MIO, S, gpt, notices, require, volume
from verifier5n import geometry, fat_files
sys.path.insert(0, str(NOVA / "boot/diagnostic-5o"))
from image5o import secteurs

LIMINE = NOVA / "third_party/limine/BOOTX64.EFI"
ROLES = ("LIMINE.EFI", "limine.conf", "NOVAOS.ELF")


TEMOINS = {"nova_start", "novaos::sched::start", "novaos::main_thread", "novaos::boot::MAP",
           "__image_start", "__text_start", "__rodata_start", "__data_start", "__image_end"}


def segments(elf):
    require(elf[:6] == b"\x7fELF\x02\x01" and elf[18:20] == b"\x3e\x00", "ELF x86_64")
    entree, debut = struct.unpack_from("<QQ", elf, 24)
    pas, nombre = struct.unpack_from("<HH", elf, 54)
    require(pas == 56 and 0 < nombre <= 64, "En-têtes de programme ELF")
    charges = []
    for i in range(nombre):
        type_, flags, offset, va, pa, taille, mem, align = struct.unpack_from("<II6Q", elf, debut + i * pas)
        if type_ == 1:
            require(offset + taille <= len(elf) and taille <= mem, "Segment ELF hors limites")
            charges.append((va, pa, flags, mem, align, elf[offset:offset + taille]))
    require(charges, "Segments ELF absents")
    return entree, charges


def preparer(out, noyau, outil, chargeur, video=False):
    temoins = TEMOINS | ({"NOVA6A_JOURNAL", "NOVA6A_EVENEMENTS", "novaos::boot::MP", "novaos::boot::CMDLINE", "novaos::boot::FRAMEBUFFER", "novaos::boot::IMAGE",
                         "novaos::smp::launch", "novaos::smp::shutdown"} if video else set())
    out.mkdir(parents=True, exist_ok=False)
    attendu = (LIMINE.parent / "SHA256SUMS").read_text().splitlines()
    require(sha256(LIMINE.read_bytes()).hexdigest() + "  BOOTX64.EFI" in attendu, "Limine officiel")
    def noms(mode):
        return subprocess.check_output(["llvm-nm", "--no-sort", "--defined-only", mode, noyau],
                                       text=True).splitlines()
    gardes = {}
    bruts, lisibles = noms("--no-demangle"), noms("--demangle")
    require(len(bruts) == len(lisibles), "Tables de symboles discordantes")
    for brut, lisible in zip(bruts, lisibles):
        addr, typ, nom = lisible.split(maxsplit=2)
        if nom in temoins:
            raw = brut.split(maxsplit=2)
            require(raw[:2] == [addr, typ] and nom not in gardes, "Symbole témoin ambigu")
            gardes[nom] = raw[2]
    require(set(gardes) == temoins, "Symbole témoin manquant")
    subprocess.run(["llvm-objcopy", "--strip-all", *["--keep-symbol=" + s for s in gardes.values()],
                    noyau, out / ROLES[2]], check=True)
    elf = (out / ROLES[2]).read_bytes()
    require(segments(noyau.read_bytes()) == segments(elf), "Image mémoire du noyau modifiée")
    conf = ("timeout: 0\nserial: yes\neditor_enabled: no\nhash_mismatch_panic: yes\n"
            "\n/NovaOS Live\n    protocol: limine\n    path: boot():/NOVAOS.ELF#"
            + blake2b(elf).hexdigest() + "\n    cmdline: smp-test\n").encode("ascii")
    (out / ROLES[1]).write_bytes(conf)
    shutil.copyfile(chargeur, out / ROLES[0])
    subprocess.run([outil, "enroll-config", out / ROLES[0], blake2b(conf).hexdigest()], check=True)
    plan = bytearray(b"NOVA5Z01")
    for nom in ROLES:
        b = (out / nom).read_bytes()
        plan.extend(struct.pack("<I32s", len(b), sha256(b).digest()))
    (out / "PLAN.BIN").write_bytes(plan)
    legal = out / "TIERS.TXT"
    subprocess.run([sys.executable, NOVA / "scripts/licences_service.py", "x86_64-unknown-none", legal], check=True)
    with legal.open("ab") as f:
        for p in sorted((NOVA / "licenses").glob("*.txt")):
            f.write(b"\n" + p.name.encode() + b"\n" + p.read_bytes())
        f.write(b"\nNovaOS\n" + (NOVA / "LICENSE").read_bytes())
        source = chargeur.parent.parent
        for name in ("3RDPARTY.md", "flanterm/LICENSE", "freestnd-c-hdrs/LICENSE",
                     "limine-protocol/LICENSE", "libfdt/BSD-2-Clause", "picoefi/COPYING",
                     "picoefi/LICENSES/LicenseRef-scancode-bsd-no-disclaimer-unmodified.txt"):
            f.write(b"\nLimine " + name.encode() + b"\n" + (source / name).read_bytes())
        stb = (source / "common/lib/stb_image.h").read_bytes()
        f.write(b"\nstb_image\n" + stb[stb.index(b"ALTERNATIVE A - MIT License"):])


def admettre(support, attendu):
    """Contrôle hôte du disque livré, incluant le premier PE et les deux GPT."""
    with os.fdopen(os.open(support, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as f:
        require(stat.S_ISREG(os.fstat(f.fileno()).st_mode), "Fichier ordinaire attendu")
        with mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as disk:
            require(sha256(disk).hexdigest() == attendu, "Empreinte du support")
            parts = geometry(disk)
            esp = fat_files(disk, parts["NOVAOS"], "NOVAOS")
            require(set(esp) == {*ROLES, "PLAN.BIN", "TIERS.TXT", "LICENCES.TXT",
                                 "EFI/BOOT/BOOTX64.EFI"}, "Liste des composants de l’ESP")
            logs = fat_files(disk, parts["NOVALOGS"], "NOVALOGS")
            require(len(logs) == 24 and all(not any(v) for v in logs.values()), "NOVALOGS")


def construire(out, support, *, defaut=None, octets=MIN_MIO * MIO):
    fichiers = {nom: (out / nom).read_bytes() for nom in (*ROLES, "PLAN.BIN", "TIERS.TXT")}
    if defaut is not None:
        b = bytearray(fichiers[defaut])
        b[-1] ^= 1
        fichiers[defaut] = bytes(b)
    pe = (out / "BOOTX64.EFI").read_bytes()
    entries = [(b"LIMINE  EFI", None, fichiers[ROLES[0]]),
               (b"LIMINE~1CON", ROLES[1], fichiers[ROLES[1]]),
               (b"NOVAOS  ELF", None, fichiers[ROLES[2]]),
               (b"PLAN    BIN", None, fichiers["PLAN.BIN"]),
               (b"TIERS   TXT", None, fichiers["TIERS.TXT"])]
    blocks = gpt(secteurs(octets))
    blocks.update(volume(ESP, "NOVAOS", pe, fichiers_racine=entries)[0])
    blocks.update(volume(LOG, "NOVALOGS")[0])
    with support.open("xb") as f:
        f.truncate(octets)
        for lba, data in sorted(blocks.items()):
            f.seek(lba * S)
            f.write(data)
    with support.open("rb") as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as disk:
        parts = geometry(disk)
        esp = fat_files(disk, parts["NOVAOS"], "NOVAOS")
        logs = fat_files(disk, parts["NOVALOGS"], "NOVALOGS")
        require(esp == dict(fichiers, **{"EFI/BOOT/BOOTX64.EFI": pe, "LICENCES.TXT": notices()}), "ESP")
        require(len(logs) == 24 and all(not any(v) for v in logs.values()), "NOVALOGS")
        print(sha256(disk).hexdigest(), support.name)


if __name__ == "__main__":
    sub = argparse.ArgumentParser(description=__doc__)
    op = sub.add_subparsers(dest="action", required=True)
    prep = op.add_parser("preparer")
    for key in ("stage", "noyau", "enroll", "chargeur"):
        prep.add_argument(key, type=Path)
    prep.add_argument("--video-6a", action="store_true")
    img = op.add_parser("assembler")
    img.add_argument("stage", type=Path)
    img.add_argument("image", type=Path)
    img.add_argument("--octets", type=int, default=MIN_MIO * MIO)
    ver = op.add_parser("verifier")
    ver.add_argument("image", type=Path)
    ver.add_argument("--sha256", required=True)
    args = sub.parse_args()
    if args.action == "preparer":
        preparer(args.stage, args.noyau, args.enroll.resolve(), args.chargeur, args.video_6a)
    elif args.action == "assembler":
        construire(args.stage, args.image, octets=args.octets)
    else:
        admettre(args.image, args.sha256)
