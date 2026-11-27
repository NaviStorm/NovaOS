"""Autorité externe, limitée à une VM Q35 neuve créée ici. Aucun périphérique hôte."""

import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import socket
import struct
import subprocess
import sys
import shutil
import time
import uuid

NOVA = Path(__file__).resolve().parents[2]
sys.path[:0] = [
    str(NOVA / "boot/diagnostic-5i"),
    str(NOVA / "boot/diagnostic-5e"),
    str(NOVA / "tests/ethernet_v1"),
]
import banc as ancien
from sonde import devices, hmp, state
from domaines_rom import construire, verifier_domaines
from inventaire import couverture, dmar
from port_qemu import qualification

FIRMWARE = "1f767a982e7af4702235e957763ab052f29ae0bdf9f84ee08f03379ef82a70b7"
MODELE = "085ed32ca433172258c779258f435cb44388e5e6f64d9d8fd816c1fd4f27791e"
PAGE = 4096


def exige(condition, raison):
    if not condition:
        raise ValueError(raison)


def empreinte(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def preparer(out, sources):
    modele = NOVA / "build/modele5j/build/qemu-system-x86_64"
    attendu = MODELE
    if sources is not None:
        # Une reconstruction part obligatoirement des sources épinglées.
        cible = out / "modele-reconstruit"
        subprocess.run(
            ["sh", Path(__file__).with_name("modele.sh"), sources.resolve(), cible],
            check=True,
        )
        modele = cible / "build/qemu-system-x86_64"
        attendu = empreinte(modele)
    fw = NOVA / "build/ovmf5j-b/Build/OvmfX64/RELEASE_CLANGDWARF/FV/OVMF_CODE.fd"
    exige(empreinte(modele) == attendu and empreinte(fw) == FIRMWARE, "Provenance PIO")
    # Page absente du contenu utile du firmware épinglé. La copie pflash reste RO.
    flash = bytearray(fw.read_bytes())
    pos = next(
        i for i in range(0, len(flash), PAGE) if flash[i : i + PAGE] == b"\xff" * PAGE
    )
    flash[pos : pos + PAGE] = bytes(PAGE)
    (out / "code.fd").write_bytes(flash)
    root = (1 << 32) - len(flash) + pos
    supports = NOVA / "build/supports5j-b"
    references = {
        "nominal.fd": "e04b60150458454c7f9608209ac7358058c2bb566ddf254e455330c065b1411b",
        "inactif.fd": "8c09b326b45951f539619b4d0f33e5e8fdbafa1cca9b173468f9df7e4a7725e0",
        "diagnostic.img": "b88f9f4a8f84c84981d0e803757c0623ac504c9b8fb88ef7af21500c7370a33a",
        "paquet.img": "51d6d5052b070b6e0134e76ff395e0159f62b19cd034c7364a50c0ff4cd38554",
        "revoque.fd": "eb18f45bf1b1d7be10e1a52d51ea898753f5b2ed1a5f65b22281e4290a8a7631",
    }
    for nom, sha in references.items():
        exige(empreinte(supports / nom) == sha, "Lot signé " + nom)
        shutil.copyfile(supports / nom, out / nom)
        if nom.endswith(".img"):
            with (out / nom).open("ab") as f:
                f.truncate(34 << 20)
    return modele, root


def commande(stream, cmd):
    stream.write((cmd + "\n").encode())
    line = stream.readline().decode().split()
    exige(line and line[0] == "OK", "qtest " + cmd.split()[0])
    return line[1] if len(line) == 2 else None


def profil(modele, root, out, mode, scenario):
    folder = out / (mode + "-" + scenario)
    folder.mkdir()
    os.chdir(folder)
    disk = Path("interne.raw")
    with disk.open("xb") as f:
        f.write(hashlib.sha256(b"NovaOS 5J namespace synthetique").digest() * 128)
        f.truncate(4 << 20)
    avant = empreinte(disk)
    epoch = str(uuid.uuid4())
    payload = "paquet" if scenario == "paquet" else "diagnostic"
    Path("epoque.bin").write_bytes(b"NOVA5J-BOOT-EPOC" + uuid.UUID(epoch).bytes)
    dry = scenario in ("absente", "inactive")
    refus = scenario in (
        "droits",
        "contradiction",
        "proprietaire",
        "en-vol",
        "table-inconnue",
    )
    firmware = out / "code.fd"
    if not dry:
        calibration = json.loads((out / "nominal-inactive.json").read_text())[
            "inventaire"
        ]
        descriptions = construire(firmware, root, calibration, scenario)
        firmware = Path("domaines.fd").resolve()
    args = [
        str(modele),
        "-machine",
        "q35,smm=on,sata=off",
        "-accel",
        "tcg",
        "-cpu",
        "max",
        "-smp",
        "1",
        "-m",
        "256",
        "-S",
        "-nodefaults",
        "-display",
        "none",
        "-no-reboot",
        "-no-shutdown",
        "-monitor",
        "none",
        "-qmp",
        "stdio",
        "-qtest",
        "unix:qt,server=on,wait=off",
        "-qtest-log",
        "/dev/null",
        "-chardev",
        "file,id=dbg,path=firmware.log",
        "-device",
        "isa-debugcon,iobase=0x402,chardev=dbg",
        "-debugcon",
        "file:efi.log",
        "-global",
        "isa-debugcon.iobase=0xe9",
        "-global",
        "driver=cfi.pflash01,property=secure,value=on",
        "-global",
        "fw_cfg_io.dma_enabled=off",
        "-nic",
        "none",
        "-device",
        "edu,id=sonde",
        "-netdev",
        "hubport,id=n,hubid=17",
        "-device",
        "virtio-net-pci,id=nic,netdev=n,iommu_platform=on,ats=off,disable-legacy=on,romfile=",
        "-device",
        "nvme,id=stock,drive=interne,serial=NOVA5J",
        "-drive",
        "if=none,id=interne,format=raw,file=interne.raw",
        "-drive",
        f"if=pflash,unit=0,format=raw,readonly=on,file={firmware}",
        "-drive",
        f"if=pflash,unit=1,format=raw,snapshot=on,file={out}/{mode}.fd",
        "-fw_cfg",
        f"name=opt/novaos/boot.fat,file={out}/{payload}.img",
        "-fw_cfg",
        "name=opt/novaos/epoch,file=epoque.bin",
    ]
    args += ["-d", "int"]
    if scenario != "absente":
        args += ["-device", "intel-iommu,aw-bits=48,device-iotlb=off"]
    for event in (
        "apic_register_write",
        "fw_cfg_select",
        "vtd_reg_write_gcmd",
        "vtd_reg_dmar_root",
        "vtd_dmar_enable",
        "vtd_dmar_fault",
    ):
        args += ["-trace", event]
    result = dict(
        modele_sha256=empreinte(modele),
        firmware_sha256=empreinte(firmware),
        epoque=epoch,
        mode=mode,
        scenario=scenario,
        racine=root,
    )
    with (
        open("vtd.log", "xb") as trace,
        subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=trace, bufsize=0
        ) as vm,
    ):
        try:
            q = ancien.Qmp(vm)
            result["io_reset"] = q.stats()
            exige(not any(result["io_reset"].values()), "Namespace au reset")
            debut = time.monotonic_ns()
            initial = state(q)
            buses = list(devices(q.call("query-pci")))
            result["inventaire"] = [
                [d["bus"], d["slot"], d["function"], d["qdev_id"]] for d in buses
            ]
            exige(all(d["bus"] == 0 for d in buses), "Alias/pont hors profil")
            units = list(initial["unites"].items())
            if scenario == "absente":
                exige(not units, "Unité inattendue")
                result["decision"] = "refus avant chargement"
            else:
                exige(len(units) == 1, "Topologie VT-d inconnue")
                addr, unit = units[0]
                base = int(addr, 16)
                exige(
                    unit["rtaddr"] == 0 and unit["gsts"] == 0,
                    "Racine héritée sans bail",
                )
                with socket.socket(socket.AF_UNIX) as sock:
                    sock.connect("qt")
                    with sock.makefile("rwb", buffering=0) as io:

                        def qt(cmd):
                            return commande(io, cmd)

                        # Témoins EDU exclusivement à l'intérieur de cette VM synthétique.
                        (edu,) = (d for d in buses if d["qdev_id"] == "sonde")
                        config = 0x80000000 | edu["slot"] << 11 | edu["function"] << 8
                        qt(f"outl 0xcf8 {config + 16}")
                        qt("outl 0xcfc 0xe0000000")
                        qt(f"outl 0xcf8 {config + 4}")
                        qt("outw 0xcfc 6")
                        exige(int(qt("inw 0xcfc"), 16) & 4, "BME EDU")
                        result["bme_avant"] = True
                        if scenario == "inactive":
                            result["decision"] = "refus avant chargement"
                        else:
                            exige(
                                result["inventaire"] == calibration,
                                "Inventaire différent de celui des baux",
                            )
                            ids = frozenset(
                                d["slot"] << 3 | d["function"] for d in buses
                            )
                            table = dmar(io)
                            Path("dmar-preliaison.bin").write_bytes(table)
                            couverture(table, base, ids)
                            result["dmar_sha256"] = hashlib.sha256(table).hexdigest()
                            result["baux"] = descriptions
                            if scenario == "en-vol":
                                qt("writel 0x5000 0x51a72ce3")
                                qt("writeq 0xe0000080 0x40000")
                                qt("writeq 0xe0000088 0x5000")
                                qt("writeq 0xe0000090 4")
                                qt("writeq 0xe0000098 3")
                                cmd = int(qt("readq 0xe0000098"), 16)
                                exige(
                                    cmd & 1
                                    and int(qt("readl 0x5000"), 16) == 0x51A72CE3,
                                    "Commande EDU en attente",
                                )
                                exige(
                                    state(q)["unites"][addr] == unit
                                    and not any(q.stats().values())
                                    and empreinte(disk) == avant,
                                    "Quarantaine avant activation",
                                )
                                result.update(
                                    decision="quarantaine avant chargement",
                                    commande_edu=cmd,
                                    traduction_avant=unit["gsts"],
                                    namespace_sha256=avant,
                                    io=q.stats(),
                                )
                                return result
                            instant_activation = time.monotonic_ns()
                            qt(f"writeq {base + 32} {root}")
                            qt(f"writel {base + 24} 0x40000000")
                            exige(int(qt(f"readl {base + 28}"), 16) & 1 << 30, "RTPS")
                            qt(f"writel {base + 24} 0x80000000")
                            status = int(qt(f"readl {base + 28}"), 16)
                            exige(status & 1 << 31, "TES")
                            actives = state(q)["unites"][addr]
                            result["vtd_avant"] = actives
                            try:
                                octets = verifier_domaines(
                                    base,
                                    root,
                                    descriptions,
                                    qt,
                                    actives,
                                    ids,
                                    uuid.UUID(epoch).bytes,
                                    Path("epoque.bin").read_bytes()[16:],
                                    scenario,
                                    "avant",
                                )
                            except ValueError as err:
                                attendus = {
                                    "table-inconnue": "Couverture RID",
                                    "droits": "Permissions ou mappings",
                                    "contradiction": "Cadre de données",
                                    "proprietaire": "Quarantaine",
                                    "en-vol": "Quarantaine",
                                }
                                if not refus or not str(err).startswith(
                                    attendus[scenario]
                                ):
                                    raise
                                exige(
                                    not q.call("query-status")["running"]
                                    and not any(q.stats().values())
                                    and empreinte(disk) == avant,
                                    "Refus non isolé",
                                )
                                result.update(
                                    decision="quarantaine avant chargement",
                                    motif=str(err),
                                    namespace_sha256=avant,
                                    io=q.stats(),
                                )
                                return result
                            exige(not refus, "Admission indue")
                            result.update(
                                tables_octets=octets,
                                etablissement_ns=time.monotonic_ns() - debut,
                                activation_controle_ns=time.monotonic_ns()
                                - instant_activation,
                            )
                            # Canari dans la RAM basse du banc avant toute copie du PE.
                            # Le modèle EDU termine sa commande sur l'horloge virtuelle.
                            qt("writel 0x5000 0x51a72ce3")
                            qt("writeq 0xe0000080 0x40000")
                            qt("writeq 0xe0000088 0x5000")
                            qt("writeq 0xe0000090 4")
                            qt("writeq 0xe0000098 3")
                            result["io_avant"] = q.stats()
                            instant = time.monotonic_ns()
                            q.call("cont")
                            fin = time.monotonic() + 30
                            while time.monotonic() < fin:
                                log = Path("efi.log").read_bytes()
                                if (
                                    b"NOVA 5J transfert=" in log
                                    or b"NOVA PIO terminal=" in log
                                ):
                                    break
                                time.sleep(0.025)
                            time.sleep(0.2)
                            q.call("stop")
                            result["reprise_observation_ns"] = (
                                time.monotonic_ns() - instant
                            )
                            Path("cpu.log").write_text(hmp(q, "info registers"))
                            result["efi"] = Path("efi.log").read_text()

                            etat_fin = state(q)
                            result["commandes_pci_fin"] = {
                                rid: row["cmd"] for rid, row in etat_fin["pci"].items()
                            }
                            apres = etat_fin["unites"][addr]
                            result["vtd_apres"] = apres
                            exige(
                                apres["rtaddr"] == root and apres["gsts"] & 1 << 31,
                                "Continuité VT-d",
                            )
                            exige(
                                verifier_domaines(
                                    base,
                                    root,
                                    descriptions,
                                    qt,
                                    apres,
                                    ids,
                                    uuid.UUID(epoch).bytes,
                                    Path("epoque.bin").read_bytes()[16:],
                                    scenario,
                                    "apres",
                                )
                                == octets,
                                "Baux après transfert",
                            )
                            result["canari"] = int(qt("readl 0x5000"), 16)
                            result["io_apres"] = q.stats()
                            exige(
                                result["io_avant"] == result["io_apres"], "E/S interne"
                            )
                            result["decision"] = "transfert à examiner"
                            exige(
                                not any(result["io_apres"].values()),
                                "E/S lors du diagnostic",
                            )
                            if "LoadImage=0000000000000000" in result["efi"]:
                                q.call(
                                    "pmemsave",
                                    val=0,
                                    size=256 << 20,
                                    filename="ram.bin",
                                )
                                ram = Path("ram.bin").read_bytes()
                                Path("ram.bin").unlink()
                                positions = [
                                    m.start()
                                    for m in re.finditer(b"NOVA5J-RAM-FINAL", ram)
                                    if struct.unpack_from("<Q", ram, m.start() + 16)[0]
                                    == m.start()
                                ]
                                exige(
                                    len(positions) == 1,
                                    "Reçu RAM terminal absent ou ambigu",
                                )
                                images = []
                                for off in range(0, positions[0], 4096):
                                    if ram[off : off + 2] != b"MZ":
                                        continue
                                    pe = (
                                        off + struct.unpack_from("<I", ram, off + 60)[0]
                                    )
                                    if ram[pe : pe + 4] != b"PE\0\0":
                                        continue
                                    taille = struct.unpack_from("<I", ram, pe + 80)[0]
                                    if off <= positions[
                                        0
                                    ] < off + taille and off + taille <= len(ram):
                                        images.append((off, off + taille))
                                exige(len(images) == 1, "Image du rapport")
                                result["plage_image"] = images[0]
                                Path("recu.bin").write_bytes(
                                    ram[positions[0] : positions[0] + 80]
                                )
                                v = struct.unpack_from("<6Q16s", ram, positions[0] + 16)
                                marque = int(
                                    re.search(
                                        r"NOVA 5J transfert=([0-9a-f]+)", result["efi"]
                                    )[1],
                                    16,
                                )
                                result.update(
                                    statut=v[2],
                                    confiance=v[3],
                                    temporaire=v[4],
                                    vivants=v[5],
                                    echo=v[6].hex(),
                                    transfert_ticks=v[1] - marque,
                                )
        finally:
            vm.terminate()
            vm.wait(timeout=10)
    lignes = Path("vtd.log").read_text().splitlines()
    result["trace"] = [l for l in lignes if l.startswith("vtd_")]
    if "plage_image" in result:
        bas, haut = result["plage_image"]
        interruptions = []
        pending = None
        for line in lignes:
            ev = re.search(r"v=([0-9a-f]+).*?i=0.*?IP=[0-9a-f]+:([0-9a-f]+)", line)
            if ev:
                rip = int(ev[2], 16)
                pending = (int(ev[1], 16), rip) if bas <= rip < haut else None
            if pending and line == "apic_register_write register 0x0b = 0x0":
                interruptions.append([*pending, line])
                pending = None
        result["irq_depuis_pe_et_eoi"] = interruptions[:8]
    exige(empreinte(disk) == avant, "Namespace altéré")
    result["namespace_sha256"] = avant
    if scenario not in ("absente", "inactive"):
        exige(
            result["canari"] == 0x51A72CE3
            and any("vtd_dmar_fault" in l and "5000" in l for l in lignes),
            "Contre-épreuve DMA",
        )
        exige(
            [l for l in result["trace"] if l.startswith("vtd_dmar_enable")]
            == ["vtd_dmar_enable enable 1"],
            "Interruption de protection",
        )
        exige(
            sum("vtd_reg_dmar_root" in l for l in lignes) == 1
            and sum("vtd_reg_write_gcmd" in l for l in lignes) == 2,
            "Changement de racine",
        )
        if mode == "revoque":
            exige(
                "LoadImage=800000000000000f" in result["efi"]
                and "statut" not in result,
                "Refus EFI attendu",
            )
            result["decision"] = "refus OVMF"
        else:
            exige(
                result["confiance"] == int(mode == "nominal")
                and result["vivants"] == 0
                and result["transfert_ticks"] > 0,
                "Mode ou reçu EFI",
            )
            if scenario == "paquet":
                exige(
                    result["statut"] == 14 and result["echo"] == bytes(16).hex(),
                    "Refus interne",
                )
                result["decision"] = "refus du paquet"
            else:
                exige(
                    result["statut"] == 0 and result["echo"] == uuid.UUID(epoch).hex,
                    "Rapprochement du reçu et du bail",
                )
                result["decision"] = "transfert en RAM confirmé"
    return result


def main():
    p = argparse.ArgumentParser()
    p.add_argument("sortie", type=Path)
    p.add_argument(
        "--reconstruire-modele",
        type=Path,
        help="sources QEMU et correctif PIO épinglés",
    )
    a = p.parse_args()
    out = a.sortie.resolve()
    out.mkdir(parents=True, exist_ok=False)
    modele, root = preparer(out, a.reconstruire_modele)
    with qualification(NOVA.parents[1], {}):
        for mode, scenario in (
            ("nominal", "absente"),
            ("nominal", "inactive"),
            ("nominal", "droits"),
            ("nominal", "contradiction"),
            ("nominal", "proprietaire"),
            ("nominal", "en-vol"),
            ("nominal", "table-inconnue"),
            ("nominal", "active"),
            ("inactif", "active"),
            ("nominal", "paquet"),
            ("inactif", "paquet"),
            ("revoque", "active"),
        ):
            r = profil(modele, root, out, mode, scenario)
            (out / (mode + "-" + scenario + ".json")).write_text(
                json.dumps(r, indent=2) + "\n"
            )


if __name__ == "__main__":
    main()
