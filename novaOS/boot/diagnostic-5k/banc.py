"""Retrait individuel dans une VM Q35 neuve. Le canal de contrôle reste privé à l'hôte."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import socket
import struct
import subprocess
import sys
import time
import uuid

from restitution import Autorite, Bail

NOVA = Path(__file__).resolve().parents[2]
sys.path[:0] = [
    str(NOVA / "boot/diagnostic-5e"),
    str(NOVA / "boot/diagnostic-5i"),
    str(NOVA / "boot/diagnostic-5j"),
    str(NOVA / "tests/ethernet_v1"),
]
from banc import Qmp
from sonde import state, devices
from inventaire import dmar, couverture
from domaines import legacy
from port_qemu import qualification
from observation_apic import observer as observer_apic
from dataclasses import replace


def exiger(ok, motif):
    if not ok:
        raise ValueError(motif)


def refuse(operation):
    try:
        operation()
    except ValueError:
        return
    raise ValueError("Refus attendu")


def exact(f, sha):
    exiger(hashlib.sha256(f.read_bytes()).hexdigest() == sha, f.name)
    return f


def sauver(nom, valeur):
    Path(nom).write_text(json.dumps(valeur, indent=2) + "\n")


class Materiel:
    def __init__(self, q, io, base, reserve, rids):
        self.q, self.io, self.base, self.reserve, self.rids = q, io, base, reserve, rids
        self.bar = None

    def demande(self, cmd):
        self.io.write((cmd + "\n").encode())
        row = self.io.readline().split()
        exiger(row and row[0] == b"OK", "qtest " + cmd.split()[0])
        return row[1] if len(row) == 2 else None

    def lire(self, a, n=8):
        return int(self.demande(f"read{ {4: 'l', 8: 'q'}[n] } {a}"), 16)

    def ecrire(self, a, v, n=8):
        self.demande(f"write{ {4: 'l', 8: 'q'}[n] } {a} {v}")

    def page(self, i):
        return self.reserve + 4096 * i

    def octets(self, a, n):
        return bytes.fromhex(self.demande(f"read {a} {n}").decode().removeprefix("0x"))

    def registre(self, offset, v=None, n=8):
        if v is None:
            return self.lire(self.base + offset, n)
        self.ecrire(self.base + offset, v, n)

    def tables(self):
        return {self.page(i): self.octets(self.page(i), 4096) for i in range(14)}

    def installer(self):
        # 0 racine, 1 contexte, trois arbres de 4 pages, 4 cadres, QI et statut.
        exiger(
            all(not any(self.octets(self.page(i), 4096)) for i in range(20)),
            "Réserve occupée",
        )
        self.ecrire(self.page(0), self.page(1) | 1)
        for i, name in enumerate(("stock", "nic", "sonde")):
            sid = self.rids[name]
            p = 2 + 4 * i
            self.ecrire(self.page(1) + 16 * sid, self.page(p) | 1)
            self.ecrire(self.page(1) + 16 * sid + 8, (i + 1) << 8 | 2)
            for j in range(p, p + 3):
                self.ecrire(self.page(j), self.page(j + 1) | 3)
            self.ecrire(self.page(p + 3) + 8, self.page(14 + i) | 1)
            if name == "sonde":
                self.ecrire(self.page(p + 3) + 16, self.page(17) | 2)
        exiger(
            self.registre(32) == 0 and self.registre(28, n=4) == 0, "Unité déjà détenue"
        )
        self.registre(32, self.page(0))
        self.registre(24, 1 << 30, 4)
        exiger(self.registre(28, n=4) == 1 << 30, "RTPS")
        self.registre(0x90, self.page(18))
        self.registre(24, 1 << 26, 4)
        self.registre(24, (1 << 26) | (1 << 31), 4)
        exiger(
            self.registre(28, n=4) == 0xC4000000
            and self.registre(32) == self.page(0)
            and self.registre(0x90) == self.page(18),
            "QI et traduction",
        )
        unit = state(self.q)["unites"][hex(self.base)]
        lu = legacy(unit, self.tables(), {self.page(i) for i in range(14, 18)})
        attendu = {}
        for i, name in enumerate(("stock", "nic", "sonde")):
            feuilles = [dict(iova=4096, cadre=self.page(14 + i), droits=1)]
            if name == "sonde":
                feuilles.append(dict(iova=8192, cadre=self.page(17), droits=2))
            attendu[self.rids[name]] = dict(did=i + 1, mappings=feuilles)
        exiger(lu == attendu, "Baux et droits différents des traductions")
        self.tables_attestees = self.tables()
        return lu

    def armer_edu(self):
        sid = self.rids["sonde"]
        cf8 = int(self.demande("inl 0xcf8"), 16)
        try:
            self.demande(f"outl 0xcf8 {0x80000010 | sid << 8}")
            self.bar = int(self.demande("inl 0xcfc"), 16) & ~15
            exiger(self.bar > 0, "BAR EDU absent")
            self.demande(f"outl 0xcf8 {0x80000004 | sid << 8}")
            cmd = int(self.demande("inw 0xcfc"), 16)
            self.demande(f"outw 0xcfc {cmd | 2}")
            exiger(self.lire(self.bar, 4) & 0xFFFF == 0xED, "Identification EDU")
            self.demande(f"outw 0xcfc {cmd | 6}")
            exiger(int(self.demande("inw 0xcfc"), 16) & 6 == 6, "BME et mémoire EDU")
        finally:
            self.demande(f"outl 0xcf8 {cf8}")

    def start(self, direction):
        src, dest = (4096, 0x40000) if direction == 1 else (0x40000, 8192)
        for reg, val in ((0x80, src), (0x88, dest), (0x90, 8), (0x98, direction)):
            self.ecrire(self.bar + reg, val)
        exiger(self.lire(self.bar + 0x98) & 1, "Commande non admise")

    def attendre(self):
        self.q.call("cont")
        try:
            fin = time.monotonic() + 3
            while self.lire(self.bar + 0x98) & 1:
                if time.monotonic() > fin:
                    raise TimeoutError("Commande encore en vol")
                time.sleep(0.001)
        finally:
            self.q.call("stop")

    def enregistrer_tables(self, avant, apres):
        for nom, donnees in (
            ("avant.bin", b"".join(avant.values())),
            ("apres.bin", b"".join(apres.values())),
            ("queue.bin", self.octets(self.page(18), 4096)),
            ("wait.bin", self.octets(self.page(19), 4096)),
        ):
            Path(nom).write_bytes(donnees)

    def nettoyer(self, page):
        self.demande(f"memset {page} 4096 0")
        return self.octets(page, 4096) == bytes(4096)

    def retrait(self, autorite, epoch, faute_qi):
        sid = self.rids["sonde"]
        motif = int.from_bytes(epoch[:8], "little") or 1
        self.armer_edu()
        self.ecrire(self.page(16), motif)
        autorite.soumettre(motif)
        self.start(1)
        self.attendre()
        self.start(3)
        autorite.fermer()
        debut = time.monotonic_ns()
        en_vol = self.lire(self.bar + 0x98) & 1
        refuse(lambda: autorite.soumettre(motif))
        exiger(en_vol and self.lire(self.page(17)) == 0, "Témoin en vol absent")
        try:
            autorite.drainer()
        except ValueError:
            pass
        else:
            raise ValueError("Drainage prématuré")
        self.attendre()
        occupe = self.lire(self.bar + 0x98) & 1
        echo = self.lire(self.page(17))
        autorite.achever(occupe, echo)
        autorite.drainer()
        fin_dma = time.monotonic_ns()
        avant = self.tables()
        exiger(
            avant == self.tables_attestees and self.registre(32) == self.page(0),
            "Tables ou racine remplacées",
        )
        # Une seule feuille de lecture et une d'écriture pour EDU. Contexte privé.
        for index in (1, 2):
            self.ecrire(self.page(13) + 8 * index, 0)
        self.ecrire(self.page(1) + 16 * sid, 0)
        self.ecrire(self.page(1) + 16 * sid + 8, 0)
        cookie = int.from_bytes(epoch[8:12], "little") or 1
        entries = [
            ((sid << 32) | (3 << 16) | 0x31, 0),
            ((3 << 16) | 0xE2, 0),
            ((cookie << 32) | 0x25, self.page(19)),
        ]
        exiger(
            self.registre(0x80) == self.registre(0x88) == self.lire(self.page(19)) == 0,
            "QI déjà occupée",
        )
        if faute_qi:
            # Descripteur nul réservé dans la file privée de cette VM jetable.
            entries[1] = (0, 0)
        for i, (lo, hi) in enumerate(entries):
            self.ecrire(self.page(18) + 16 * i, lo)
            self.ecrire(self.page(18) + 16 * i + 8, hi)
        autorite.preparer_invalidation(cookie, 48)
        refuse(
            lambda: autorite.confirmer(
                self.lire(self.page(19), 4),
                self.registre(0x80),
                self.registre(0x88),
                0,
                True,
            )
        )
        refuse(lambda: autorite.restituer(None, self.nettoyer))
        debut_qi = time.monotonic_ns()
        self.registre(0x88, 48)
        tete = self.registre(0x80)
        queue = self.registre(0x88)
        marque = self.lire(self.page(19), 4)
        fin_qi = time.monotonic_ns()
        apres = self.tables()
        contexte = bytearray(avant[self.page(1)])
        contexte[16 * sid : 16 * sid + 16] = bytes(16)
        bonnes = (
            apres == avant | {self.page(1): bytes(contexte), self.page(13): bytes(4096)}
            and self.registre(32) == self.page(0)
            and self.registre(28, n=4) == 0xC4000000
        )
        fsts = self.registre(0x34, n=4)
        commun = dict(
            drain_ns=fin_dma - debut,
            invalidation_ns=fin_qi - debut_qi,
            motif=motif,
            rid=sid,
            did=3,
            cookie=cookie,
            wait=marque,
            iqh=tete,
            iqt=queue,
            commandes=entries,
            en_vol=en_vol,
            occupe_final=occupe,
            echo=echo,
            fsts_qi=fsts,
        )
        if faute_qi:
            exiger(
                bonnes and fsts & 16 and tete == 16 and queue == 48 and marque == 0,
                "Faute de file non observée",
            )
            refuse(lambda: autorite.confirmer(marque, tete, queue, fsts, bonnes))
            refuse(lambda: autorite.restituer(None, self.nettoyer))
            exiger(self.lire(self.page(17)) == motif, "Page en quarantaine altérée")
            self.enregistrer_tables(avant, apres)
            return dict(
                **commun,
                bail=autorite.etat,
                pages=[],
                retenues=list(autorite.bail.pages),
            )
        recu = autorite.confirmer(marque, tete, queue, fsts, bonnes)
        for faux in (
            None,
            replace(recu),
            replace(recu, cookie=0),
            replace(recu, bail=replace(recu.bail, generation=recu.bail.generation + 1)),
        ):
            refuse(lambda: autorite.restituer(faux, self.nettoyer))
        pages = autorite.restituer(recu, self.nettoyer)
        refuse(lambda: autorite.restituer(recu, self.nettoyer))
        nouveau = motif ^ 0xFFFFFFFFFFFFFFFF
        self.ecrire(self.page(17), nouveau)
        fin_ret = time.monotonic_ns()
        # Stimulus indépendant de la porte de soumission fermée, BME encore actif.
        self.start(3)
        self.attendre()
        exiger(
            self.lire(self.page(17)) == nouveau and self.registre(0x34, n=4) & 2,
            "Accès tardif ou absence de faute",
        )
        self.enregistrer_tables(avant, apres)
        return dict(
            **commun,
            restitution_ns=fin_ret - fin_qi,
            nouveau=nouveau,
            pages=pages,
            retenues=[],
            post_dma=self.lire(self.page(17)),
            fsts=self.registre(0x34, n=4),
            bail=autorite.etat,
        )


def arguments_q35(exe, fw, fat, var):
    args = [
        exe,
        "-machine",
        "q35,smm=on,sata=off,nvdimm=on",
        "-accel",
        "tcg",
        "-cpu",
        "max",
        "-smp",
        "1",
        "-m",
        "256M,slots=1,maxmem=512M",
        "-object",
        "memory-backend-ram,id=prive,size=2M",
        "-device",
        "nvdimm,id=labo,memdev=prive",
        "-S",
        "-nodefaults",
        "-no-reboot",
        "-no-shutdown",
        "-display",
        "none",
        "-monitor",
        "none",
        "-qmp",
        "stdio",
        "-qtest",
        "unix:qt,server=on,wait=off",
        "-qtest-log",
        "/dev/null",
        "-gdb",
        "unix:observateur.sock,server=on,wait=off",
        "-debugcon",
        "file:efi.log",
        "-global",
        "isa-debugcon.iobase=0xe9",
        "-global",
        "driver=cfi.pflash01,property=secure,value=on",
        "-global",
        "fw_cfg_io.dma_enabled=off",
        "-drive",
        f"if=pflash,unit=0,format=raw,readonly=on,file={fw}",
        "-drive",
        f"if=pflash,unit=1,format=raw,snapshot=on,file={var}",
        "-fw_cfg",
        f"name=opt/novaos/boot.fat,file={fat}",
        "-fw_cfg",
        "name=opt/novaos/epoch,file=epoch.bin",
        "-device",
        "intel-iommu,aw-bits=48,device-iotlb=off",
        "-device",
        "edu,id=sonde",
        "-nic",
        "none",
        "-netdev",
        "hubport,id=n,hubid=17",
        "-device",
        "virtio-net-pci,id=nic,netdev=n,iommu_platform=on,ats=off,disable-legacy=on,romfile=",
        "-drive",
        "if=none,id=interne,format=raw,file=interne.raw",
        "-device",
        "nvme,id=stock,drive=interne,serial=NOVA5K",
        "-d",
        "int",
    ]
    for ev in (
        "apic_local_deliver",
        "apic_deliver_irq",
        "pci_nvme_irq_pin",
        "pci_nvme_irq_msix",
        "virtio_notify",
        "apic_register_write",
        "vtd_dmar_fault",
        "vtd_reg_dmar_root",
        "vtd_dmar_enable",
        "vtd_inv_desc_cc_device",
        "vtd_inv_desc_iotlb_domain",
        "vtd_inv_desc_wait_sw",
    ):
        args.extend(["-trace", ev])
    return args


def essai(out, j, mode, observer=None, faute_qi=False):
    out.mkdir()
    os.chdir(out)
    exe = exact(
        j / "modele5j/build/qemu-system-x86_64",
        "085ed32ca433172258c779258f435cb44388e5e6f64d9d8fd816c1fd4f27791e",
    )
    fw = exact(
        j / "ovmf5j-b/Build/OvmfX64/RELEASE_CLANGDWARF/FV/OVMF_CODE.fd",
        "1f767a982e7af4702235e957763ab052f29ae0bdf9f84ee08f03379ef82a70b7",
    )
    fat = exact(
        j / "qualification5j/diagnostic.img",
        "f2783c0d4b93157fd4f46118a11ce6e4f36943f6e8c2ddb62342b1bffefcf88c",
    )
    var = exact(
        j / "qualification5j" / (mode + ".fd"),
        {
            "nominal": "e04b60150458454c7f9608209ac7358058c2bb566ddf254e455330c065b1411b",
            "inactif": "8c09b326b45951f539619b4d0f33e5e8fdbafa1cca9b173468f9df7e4a7725e0",
        }[mode],
    )
    epoch = uuid.uuid4().bytes
    Path("epoch.bin").write_bytes(b"NOVA5J-BOOT-EPOC" + epoch)
    disk = Path("interne.raw")
    disk.write_bytes(hashlib.sha256(b"namespace 5K").digest() * (131072))
    digest = hashlib.sha256(disk.read_bytes()).hexdigest()
    args = arguments_q35(exe, fw, fat, var)
    with (
        open("trace.log", "xb") as log,
        subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0
        ) as vm,
    ):
        try:
            q = Qmp(vm)
            (dimm,) = q.call("query-memory-devices")
            exiger(
                dimm["type"] == "nvdimm"
                and dimm["data"]["id"] == "labo"
                and dimm["data"]["addr"] >= 1 << 32,
                "Réserve hors RAM générale",
            )
            reserve = dimm["data"]["addr"]
            (adresse,) = state(q)["unites"]
            base = int(adresse, 16)
            pci = list(devices(q.call("query-pci")))
            exiger(all(d["bus"] == 0 for d in pci), "Pont ou alias inconnu")
            rids = {
                d["qdev_id"]: d["slot"] * 8 + d["function"] for d in pci if d["qdev_id"]
            }
            with socket.socket(socket.AF_UNIX) as s:
                s.connect("qt")
                with s.makefile("rwb", buffering=0) as io:
                    couverture(
                        dmar(io), base, {d["slot"] * 8 + d["function"] for d in pci}
                    )
                    hw = Materiel(q, io, base, reserve, rids)
                    domaines = hw.installer()
                    if observer:
                        observer(q, j)
                    else:
                        q.call("cont")
                        deadline = time.monotonic() + 12
                        while "NOVA 5J transfert=" not in Path("efi.log").read_text():
                            if time.monotonic() > deadline:
                                raise TimeoutError("Transfert EFI absent")
                            time.sleep(0.02)
                        time.sleep(0.2)
                        q.call("stop")
                    q.call("pmemsave", val=0, size=256 << 20, filename="ram.bin")
                    ram = Path("ram.bin").read_bytes()
                    Path("ram.bin").unlink()
                    (pos,) = [
                        m.start()
                        for m in re.finditer(b"NOVA5J-RAM-FINAL", ram)
                        if struct.unpack_from("<Q", ram, m.start() + 16)[0] == m.start()
                    ]
                    rec = ram[pos : pos + 80]
                    Path("recu-efi.bin").write_bytes(rec)
                    _, _, _, status, sb, _, live, echo = struct.unpack("<16s6Q16s", rec)
                    exiger(
                        status == live == 0
                        and sb == int(mode == "nominal")
                        and echo == epoch,
                        "Reçu EFI",
                    )
                    if not observer:
                        calibrer(ram, pos, j)
                    del ram
                    autorite = Autorite(
                        Bail(
                            epoch,
                            "edu-labo",
                            rids["sonde"],
                            3,
                            1,
                            1,
                            (hw.page(16), hw.page(17)),
                        )
                    )
                    avant = q.stats()
                    reg_avant = state(q)
                    bilan = hw.retrait(autorite, epoch, faute_qi)
                    reg_apres = state(q)
                    cle_sonde = f"0000:00:{rids['sonde'] // 8:02x}.{rids['sonde'] % 8}"
                    for cle, row in reg_avant["pci"].items():
                        if cle != cle_sonde:
                            exiger(
                                reg_apres["pci"][cle] == row,
                                "Configuration PCI voisine modifiée",
                            )
                    exiger(
                        reg_avant["unites"] == reg_apres["unites"],
                        "Racine, traduction ou capacité modifiée",
                    )
                    sauver("registres.json", dict(avant=reg_avant, apres=reg_apres))
                    apres = q.stats()
                    exiger(avant == apres and not any(apres.values()), "E/S interne")
            q.call("quit")
            vm.wait(timeout=5)
        finally:
            if vm.poll() is None:
                vm.terminate()
                vm.wait(timeout=5)
    trace = Path("trace.log").read_text()
    exiger(
        (
            faute_qi
            or re.search(
                rf"vtd_dmar_fault sid {rids['sonde']:#x} fault \d+ addr 0x2000 write 1",
                trace,
            )
        )
        and trace.count("vtd_dmar_enable enable 1") == 1
        and trace.count("vtd_reg_dmar_root addr") == 1,
        "Faute ou racine",
    )
    exiger(hashlib.sha256(disk.read_bytes()).hexdigest() == digest, "Namespace modifié")
    sauver(
        "resultat.json",
        dict(
            epoque=epoch.hex(),
            titulaire="edu-labo",
            incarnation=1,
            generation=1,
            rss_observateur_darwin_octets=resource.getrusage(
                resource.RUSAGE_SELF
            ).ru_maxrss,
            mode=mode,
            reserve=reserve,
            domaines=domaines,
            io=apres,
            namespace=digest,
            **bilan,
        ),
    )


def calibrer(ram, pos, j):
    image = exact(
        j / "ovmf5j-b/Build/OvmfX64/RELEASE_CLANGDWARF/X64/LocalApicTimerDxe.efi",
        "3190d6c376507df605ffb7dd99f5af4773a0ad62e26f247d4a2f67c62926b854",
    ).read_bytes()
    # Ces RVA appartiennent exclusivement à ce module OVMF épinglé.
    motif = image[0x4B9:0x544]
    matches = [
        m.start() - 0x4B9
        for m in re.finditer(re.escape(motif), ram)
        if (m.start() - 0x4B9) % 4096 == 0
        and ram[m.start() - 0x4B9 : m.start() - 0x4B9 + 2] == b"MZ"
    ]
    bases = []
    for off in range(0, pos, 4096):
        if ram[off : off + 2] != b"MZ":
            continue
        pe = off + struct.unpack_from("<I", ram, off + 60)[0]
        if ram[pe : pe + 4] == b"PE\0\0" and off <= pos < off + struct.unpack_from(
            "<I", ram, pe + 80
        )[0] <= len(ram):
            bases.append((off, off + struct.unpack_from("<I", ram, pe + 40)[0]))
    exiger(len(bases) == 1, "PE diagnostic ambigu")
    sauver(
        "calibration.json", dict(image=bases[0][0], entree=bases[0][1], timers=matches)
    )


def main():
    p = argparse.ArgumentParser()
    p.add_argument("build5j", type=Path)
    p.add_argument("sortie", type=Path)
    a = p.parse_args()
    out = a.sortie.resolve()
    j = a.build5j.resolve()
    out.mkdir(parents=True, exist_ok=False)
    with qualification(NOVA.parents[1], {}):
        for mode in ("nominal", "inactif"):
            essai(out / mode, j, mode)
            calibration = json.loads((out / mode / "calibration.json").read_text())
            essai(
                out / (mode + "-apic"),
                j,
                mode,
                lambda q, j: observer_apic(q, j, calibration),
            )
        essai(out / "iqe", j, "nominal", faute_qi=True)


if __name__ == "__main__":
    main()
