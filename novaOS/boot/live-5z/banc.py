"""Banc Q35/TCG avec USB externe en lecture seule, SATA privé et réseau isolé."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import time

from image import NOVA, admettre, construire
sys.path[:0] = [str(NOVA / "boot/diagnostic-5e"), str(NOVA / "boot/diagnostic-5i"),
                str(NOVA / "tests/ethernet_v1")]
from banc import Qmp, Debug
from hashlib import sha256
from sonde import state, hmp
from port_qemu import qualification

TRACE = ("pci_cfg_write", "vtd_reg_write", "ide_bus_exec_cmd", "ahci_cmd_done",
         "e1000e_tx_descr", "e1000e_rx_descr", "e1000e_core_write",
         "e1000e_irq_msi_notify", "e1000e_irq_msix_notify_vec", "e1000e_irq_legacy_notify")
MASQUE = 0x000ffffffffff000


def digest(p):
    h = sha256()
    with p.open("rb") as f:
        for bloc in iter(lambda: f.read(1 << 20), b""):
            h.update(bloc)
    return h.hexdigest()


def enregistrer(nom, contenu):
    Path(nom).write_text(json.dumps(contenu, indent=2, sort_keys=True) + "\n")


@contextmanager
def machine(dossier, image, fw, magasin, ram, cpus, video=True):
    dossier.mkdir()
    avant = Path.cwd()
    os.chdir(dossier)
    try:
        shutil.copyfile(magasin, "variables.fd")
        with open("interne.raw", "xb") as d:
            d.truncate(4 << 20)
        args = ["qemu-system-x86_64", "-machine", "q35,smm=on", "-accel", "tcg,thread=multi",
                "-cpu", "max", "-m", str(ram), "-smp", str(cpus), "-nodefaults",
                *(["-device", "VGA"] if video else []), "-display", "none", "-serial", "file:serie.log",
                "-qmp", "stdio", "-monitor", "none", "-S", "-no-reboot", "-no-shutdown",
                "-gdb", "unix:observateur.sock,server=on,wait=off", "-D", "transactions.log",
                "-drive", f"if=pflash,unit=0,format=raw,readonly=on,file={fw}",
                "-drive", "if=pflash,unit=1,format=raw,file=variables.fd",
                "-device", "qemu-xhci,id=usb", "-drive", f"if=none,id=externe,format=raw,readonly=on,file={image}",
                "-device", "usb-storage,drive=externe,bus=usb.0,bootindex=1",
                "-drive", "if=none,id=interne,format=raw,file=interne.raw",
                "-device", "ide-hd,drive=interne,bus=ide.0,bootindex=2",
                "-netdev", "hubport,id=prive,hubid=0", "-device", "e1000e,netdev=prive,romfile="]
        with open("qemu.log", "xb") as log, subprocess.Popen(args, stdin=subprocess.PIPE,
                stdout=subprocess.PIPE, stderr=log, bufsize=0) as proc:
            try:
                yield Qmp(proc)
            finally:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
    finally:
        os.chdir(avant)


def attendre(marque):
    fin = time.monotonic() + 45
    while time.monotonic() < fin:
        s = Path("serie.log").read_bytes()
        if marque in s:
            return s
        if b"NOVA5Z REFUS " in s or b"Panique du noyau" in s:
            break
        time.sleep(.1)
    raise ValueError("Jalon absent : " + repr(s[-550:]))


def symboles(elf, supplement=()):
    s = subprocess.check_output(["llvm-nm", "--demangle", "--defined-only", elf], text=True)
    resultat = {}
    for nom in ("nova_start", "novaos::sched::start", "novaos::main_thread",
                "novaos::boot::MAP", "__image_start", "__text_start", "__rodata_start", "__data_start", "__image_end", *supplement):
        lignes = [l for l in s.splitlines() if l.split(maxsplit=2)[-1] == nom]
        if len(lignes) != 1:
            raise ValueError("Symbole ELF absent ou ambigu : " + nom)
        resultat[nom] = int(lignes[0].split()[0], 16)
    return resultat


def gmem(d, addr, size):
    b = bytes.fromhex(d.command(f"m{addr:x},{size:x}"))
    if len(b) != size:
        raise ValueError("Lecture observateur incomplète")
    return b


def atteindre(d, addr):
    d.watch(addr)
    d.resume()
    if d.pc() != addr:
        raise ValueError("RIP différent du symbole attendu")
    d.remove(addr)


def instant(q):
    return dict(sata=q.stats(), registres=hmp(q, "info registers"), pci=state(q)["pci"])


def relais_memoire(d, syms, elf):
    """Lire la réponse de Limine avant la première instruction du noyau."""
    def mots(addr, n):
        if addr < 0xffff800000000000 or addr % 8:
            raise ValueError("Pointeur Limine hors profil")
        return struct.unpack(f"<{n}Q", gmem(d, addr, n * 8))
    reponse, = mots(syms["novaos::boot::MAP"] + 40, 1)
    _, n, ptrs = mots(reponse, 3)
    if not 0 < n <= 512:
        raise ValueError("Taille de la carte Limine")
    entrees = [mots(p, 3) for p in mots(ptrs, n)]
    if any(not size or base + size > 1 << 64 for base, size, _ in entrees):
        raise ValueError("Intervalle mémoire Limine invalide")
    if not any(kind == 5 for _, _, kind in entrees):
        raise ValueError("Mémoire du chargeur non réservée")
    buf = elf.read_bytes()
    start = syms["nova_start"]
    phoff, = struct.unpack_from("<Q", buf, 32)
    ent, count = struct.unpack_from("<HH", buf, 54)
    for i in range(count):
        typ, _, off, va, _, size, _, _ = struct.unpack_from("<II6Q", buf, phoff + i * ent)
        if typ == 1 and va <= start and start + 32 <= va + size:
            pos = off + start - va
            if gmem(d, start, 32) == buf[pos:pos + 32]:
                return entrees
    raise ValueError("Instructions d’entrée différentes de l’ELF")


def controle_pages(q, syms, ram, entrees):
    """Parcours indépendant des tables CPU du BSP via la RAM physique QEMU."""
    regs = hmp(q, "info registers")
    racine = int(re.search(r"CR3=([a-fA-F0-9]+)", regs)[1], 16) & MASQUE
    cr0 = int(re.search(r"CR0=([a-fA-F0-9]+)", regs)[1], 16)
    efer = int(re.search(r"EFER=([a-fA-F0-9]+)", regs)[1], 16)
    if not cr0 & (1 << 16) or not efer & (1 << 11):
        raise ValueError("WP ou NXE inactif")
    tables = {}
    def table(pa):
        if not 0 < pa < (ram << 20):
            raise ValueError("Table hors RAM")
        if pa not in tables:
            f = Path(f"pt-{pa:x}.bin")
            q.call("pmemsave", val=pa, size=4096, filename=f.name)
            tables[pa] = struct.unpack("<512Q", f.read_bytes())
        return tables[pa]
    groupes = {}
    physiques = set()
    for va in range(syms["__image_start"], syms["__image_end"], 4096):
        pa, rw, user, nx = racine, True, True, False
        for shift in (39, 30, 21, 12):
            e = table(pa)[va >> shift & 511]
            if not e & 1 or (shift > 12 and e & 128):
                raise ValueError("Page absente ou grande page imprévue")
            rw, user, nx = rw and bool(e & 2), user and bool(e & 4), nx or bool(e >> 63)
            pa = e & MASQUE
        groupe = ("requetes" if va < syms["__text_start"] else
                  "code" if va < syms["__rodata_start"] else
                  "constantes" if va < syms["__data_start"] else "donnees")
        if user or rw != (groupe == "donnees") or nx != (groupe != "code"):
            raise ValueError(f"Droits inattendus {groupe} {va:x} RW={rw} NX={nx} U={user}")
        if pa >= ram << 20 or pa in physiques:
            raise ValueError("Cadre du noyau hors RAM ou dupliqué")
        classes = [k for base, taille, k in entrees if base < pa + 4096 and pa < base + taille]
        if classes != [6] or not any(base <= pa and pa + 4096 <= base + taille and k == 6
                                    for base, taille, k in entrees):
            raise ValueError("Cadre noyau non réservé dans la carte Limine")
        physiques.add(pa)
        groupes[groupe] = groupes.get(groupe, 0) + 1
    if physiques.intersection(tables):
        raise ValueError("Cadres des tables confondus avec l’image ELF")
    return dict(cr3=racine, wp=True, nxe=True, groupes=groupes, tables=sorted(tables))


def essai(out, stage, fw, magasin, image, ram, cpus, mesure=False, video=None):
    fin = f"SMP rendezvous={((1 << cpus) - 1):#x} actifs={cpus} OK".encode()
    opts = (image, fw, magasin, ram, cpus)
    with machine(out / f"cal-{ram}-{cpus}", *opts) as q:
        q.call("cont")
        s = attendre(fin)
        entree = int(re.search(rb"NOVA5Z EFI entree=([0-9a-f]+)", s)[1], 16)
    if not mesure:
        return entree
    syms = symboles(stage / "NOVAOS.ELF", ("NOVA6A_JOURNAL", "NOVA6A_EVENEMENTS", "novaos::boot::FRAMEBUFFER", "novaos::smp::launch", "novaos::smp::shutdown") if video else ())
    with machine(out / f"nominal-{ram}-{cpus}", *opts) as q:
        d = Debug()
        try:
            etats = {"reset": q.stats()}
            canari = digest(Path("interne.raw"))
            atteindre(d, entree)
            etats["efi"] = instant(q)
            registres = bytes.fromhex(d.command("g"))
            st = struct.unpack_from("<Q", registres, 24)[0]
            if gmem(d, st, 8) != b"IBI SYST":
                raise ValueError("Première instruction EFI non confirmée")
            bs = struct.unpack("<Q", gmem(d, st + 96, 8))[0]
            ebs = struct.unpack("<Q", gmem(d, bs + 24 + 26 * 8, 8))[0]
            for e in TRACE: q.call("trace-event-set-state", name=e, enable=True)
            atteindre(d, ebs)
            etats["exit_boot_services"] = instant(q)
            r = bytes.fromhex(d.command("g"))
            key = struct.unpack_from("<Q", r, 24)[0]
            rsp = struct.unpack_from("<Q", r, 56)[0]
            retour = struct.unpack("<Q", gmem(d, rsp, 8))[0]
            atteindre(d, retour)
            if int.from_bytes(bytes.fromhex(d.command("g"))[:8], "little") != 0:
                raise ValueError("ExitBootServices a échoué")
            etats["retour_ebs"] = instant(q)
            atteindre(d, syms["nova_start"])
            etats["nova_start"] = instant(q)
            if video:
                video("entree", q, d, syms)
            carte = relais_memoire(d, syms, stage / "NOVAOS.ELF")
            atteindre(d, syms["novaos::sched::start"])
            etats["apres_memoire"] = instant(q)
            if video:
                video("memoire", q, d, syms)
            pages = controle_pages(q, syms, ram, carte)
            atteindre(d, syms["novaos::main_thread"])
            etats["tache_principale"] = instant(q)
            if video:
                video("premiere-tache", q, d, syms)
                atteindre(d, syms["novaos::smp::launch"])
                video("ordonnanceur", q, d, syms)
                atteindre(d, syms["novaos::smp::shutdown"])
                video("smp", q, d, syms)
            q.call("cont")
            attendre(fin)
            attendre(b"SMP arret motif=normal")
            if video:
                attendre(b"NOVA6A journal=")
                time.sleep(.15)
            q.call("stop")
            if video:
                video("terminal", q, d, syms)
            etats["rendezvous"] = instant(q)
            for e in TRACE: q.call("trace-event-set-state", name=e, enable=False)
            if canari != digest(Path("interne.raw")):
                raise ValueError("Disque interne modifié")
            if any(v["sata"] != etats["efi"]["sata"] for k, v in etats.items() if k != "reset"):
                raise ValueError("Activité du stockage interne depuis l’entrée EFI")
            for k, v in etats.items():
                if k == "reset":
                    continue
                if v["pci"] != etats["efi"]["pci"]:
                    raise ValueError("État des fonctions PCI modifié")
            row = dict(carte_limine=carte, ram_mio=ram, cpus=cpus, sha256_image=digest(image),
                       sha256_ovmf=digest(fw), sha256_variables_initiales=digest(magasin),
                       entree_efi=entree, ebs=ebs, retour_ebs=retour, map_key=key,
                       statut_ebs=0, symboles=syms, pages=pages, etats=etats, canari=canari)
            enregistrer("mesures.json", row)
            return row
        finally:
            d.sock.close()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("build", type=Path)
    p.add_argument("sortie", type=Path)
    p.add_argument("--firmware", type=Path, required=True)
    p.add_argument("--variables", type=Path, required=True)
    p.add_argument("--mode", choices=("calibrer", "qualifier", "refus"), required=True)
    p.add_argument("--ram", type=int, choices=(192, 256, 384, 512), default=192)
    p.add_argument("--cpus", type=int, choices=(1, 2, 4), default=1)
    p.add_argument("--sha256", help="Empreinte de publication indépendante de l’image présentée")
    a = p.parse_args()
    out, b, fw, var = [x.resolve() for x in (a.sortie, a.build, a.firmware, a.variables)]
    if a.mode != "refus":
        admettre(b / "novaos-live.img", a.sha256)
    out.mkdir(parents=True, exist_ok=False)
    with qualification(NOVA.parents[1], {}):
        if a.mode != "refus":
            essai(out, b / "stage", fw, var, b / "novaos-live.img", a.ram, a.cpus, a.mode == "qualifier")
        else:
            for role, code in (("LIMINE.EFI","LIMINE"), ("limine.conf","CONFIG"), ("NOVAOS.ELF","NOYAU"), ("PLAN.BIN","MANIFESTE")):
                img = out / (code + ".img")
                construire(b / "stage", img, defaut=role)
                with machine(out / code, img, fw, var, a.ram, a.cpus) as q:
                    q.call("cont")
                    s = attendre(("NOVA5Z REFUS " + code).encode())
                    time.sleep(.3)
                    q.call("stop")
                    regs = hmp(q, "info registers")
                    if b"lancement Limine" in s or "HLT=1" not in regs:
                        raise ValueError("Refus incomplet")
                    enregistrer("refus.json", dict(motif=code, image=digest(img), sata=q.stats(), registres=regs))


if __name__ == "__main__":
    main()
