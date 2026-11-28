"""OVMF distribué, disques QEMU ordinaires. Observation indépendante de l’EFI."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import time

NOVA = Path(__file__).resolve().parents[2]
sys.path[:0] = [
    str(NOVA / "boot/diagnostic-5i"),
    str(NOVA / "boot/diagnostic-5e"),
    str(NOVA / "tests/ethernet_v1"),
]
from banc import Qmp, Debug, digest
from sonde import state, hmp
from port_qemu import qualification
import temoin_vtd

RAM = 128 << 20
EV = (
    "apm_io_write",
    "pci_cfg_write",
    "vtd_reg_write",
    "ide_bus_exec_cmd",
    "ahci_cmd_done",
    "usb_msd_cmd_submit",
    "e1000e_tx_descr",
    "e1000e_rx_descr",
)


def bornes(q):
    q.call("pmemsave", val=0, size=RAM, filename="ram.bin")
    p = Path("ram.bin")
    ram = p.read_bytes()
    p.unlink()

    def adresse(magic):
        matches = [
            m.start()
            for m in re.finditer(magic, ram)
            if struct.unpack_from("<Q", ram, m.start() + 16)[0] == m.start()
        ]
        if len(matches) != 1:
            raise ValueError("Marque terminale absente ou ambigue")
        return matches[0]

    pos = adresse(b"NOVA5W-TERMINAL!")
    debut, fin = struct.unpack_from("<2Q", ram, pos + 24)
    journal = adresse(b"NOVA5I-RAM-TRACE")
    n = struct.unpack_from("<Q", ram, journal + 24)[0]
    if not (0 < n <= 24576 and 0 < debut < RAM and 0 < fin < RAM):
        raise ValueError("Journal ou bornes hors RAM")
    return (debut, fin), ram[journal + 32 : journal + 32 + n].decode("ascii")


def etat(q):
    return dict(interne=q.stats(), materiel=state(q), variables=digest(Path("vars.fd")))


def essai(
    dossier,
    fw,
    var,
    img,
    vtd,
    refus_fw=False,
    bornes_prevues=None,
    actif_vtd=False,
    inconnu=False,
):
    dossier.mkdir()
    origine = Path.cwd()
    os.chdir(dossier)
    try:
        shutil.copyfile(var, "vars.fd")
        with open("interne.raw", "xb") as disk:
            disk.truncate(4 << 20)
        canari = digest(Path("interne.raw"))
        args = [
            "qemu-system-x86_64",
            "-machine",
            "q35,smm=on",
            "-accel",
            "tcg,thread=single",
            "-cpu",
            "max",
            "-smp",
            "2",
            "-m",
            "128M",
            "-nodefaults",
            "-device",
            "VGA",
            "-display",
            "none",
            "-serial",
            "file:serie.log",
            "-monitor",
            "none",
            "-qmp",
            "stdio",
            "-S",
            "-no-reboot",
            "-no-shutdown",
            "-gdb",
            "unix:observateur.sock,server=on,wait=off",
            "-D",
            "transactions.log",
            "-drive",
            f"if=pflash,unit=0,format=raw,readonly=on,file={fw}",
            "-drive",
            "if=pflash,unit=1,format=raw,file=vars.fd",
            "-global",
            "driver=cfi.pflash01,property=secure,value=on",
            "-device",
            "qemu-xhci,id=usb",
            "-drive",
            f"if=none,id=externe,format=raw,readonly=on,file={img}",
            "-device",
            "usb-storage,drive=externe,bus=usb.0,bootindex=1",
            "-drive",
            "if=none,id=interne,format=raw,file=interne.raw",
            "-device",
            "ide-hd,drive=interne,bus=ide.0,bootindex=2",
            "-netdev",
            "hubport,id=isole,hubid=0",
            "-device",
            "e1000e,netdev=isole,romfile=",
        ]
        if actif_vtd:
            Path("temoin.aml").write_bytes(temoin_vtd.description())
            args.extend(
                [
                    "-acpitable",
                    "file=temoin.aml",
                    "-qtest",
                    "unix:vtd-temoin.sock,server=on,wait=off",
                    "-qtest-log",
                    "qtest.log",
                ]
            )
        if inconnu:
            args.extend(["-device", "pci-testdev"])
        if vtd:
            args.extend(["-device", "intel-iommu"])
        with open("qemu.log", "xb") as log, subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0
        ) as proc:
            debug = None
            try:
                q = Qmp(proc)
                zero = q.stats()
                stimulus = None
                if bornes_prevues:
                    debut, fin = bornes_prevues
                    debug = Debug()
                    debug.watch(debut)
                    debug.watch(fin)
                    debug.resume()
                    if debug.pc() != debut:
                        raise ValueError("Première instruction EFI non atteinte")
                    debug.remove(debut)
                    if actif_vtd:
                        stimulus = temoin_vtd.installer(q)
                    entree = etat(q)
                    for event in EV:
                        q.call("trace-event-set-state", name=event, enable=True)
                    debug.resume()
                    if debug.pc() != fin:
                        raise ValueError("Arrêt attendu non atteint")
                    debug.remove(fin)
                    q.call("cont")
                    time.sleep(0.35)
                    q.call("stop")
                    for event in EV:
                        q.call("trace-event-set-state", name=event, enable=False)
                else:
                    q.call("cont")
                    deadline = time.monotonic() + 35
                    while time.monotonic() < deadline:
                        serie = Path("serie.log").read_bytes()
                        regs = hmp(q, "info registers")
                        if b"No bootable option" in serie or (
                            "HLT=1" in regs
                            and re.search(r"RFL=([0-9a-fA-F]+)", regs)
                            and int(re.search(r"RFL=([0-9a-fA-F]+)", regs)[1], 16)
                            & 0x200
                            == 0
                            and b"STANDARD" in serie
                        ):
                            break
                        time.sleep(0.25)
                    q.call("stop")
                apres = etat(q)
                q.call("screendump", filename="ecran.ppm")
                if refus_fw:
                    serie = Path("serie.log").read_bytes()
                    if b"STANDARD" in serie or not any(
                        b"USB" in line
                        and (b"Access Denied" in line or b"Security Violation" in line)
                        for line in serie.splitlines()
                    ):
                        raise ValueError("Refus USB non attribue")
                    repli = any(
                        b"Sata(" in l and b"Not Found" in l for l in serie.splitlines()
                    )
                    if (
                        not repli
                        or b"No bootable option" not in serie
                        or canari != digest(Path("interne.raw"))
                    ):
                        raise ValueError("Repli et intégrité du canari non établis")
                    return dict(
                        refus_ovmf=True,
                        repli_sata=True,
                        aucune_option=True,
                        avant=zero,
                        apres=apres["interne"],
                        disque_intact=canari == digest(Path("interne.raw")),
                    )
                limites, journal = bornes(q)
                Path("journal.txt").write_text(journal)
                regs = hmp(q, "info registers")
                rip = int(re.search(r"RIP=([0-9a-fA-F]+)", regs)[1], 16)
                if "HLT=1" not in regs or not limites[1] <= rip < limites[1] + 16:
                    raise ValueError("Arrêt CPU de NovaOS non établi")
                if not bornes_prevues:
                    return limites
                if (
                    limites != bornes_prevues
                    or entree != apres
                    or canari != digest(Path("interne.raw"))
                ):
                    raise ValueError(
                        "Modification interne, PCI, VT-d, variables ou bornes"
                    )
                trace = Path("transactions.log").read_text().splitlines()
                compteurs = {e: sum(l.startswith(e + " ") for l in trace) for e in EV}
                if any(n for e, n in compteurs.items() if e != "apm_io_write"):
                    raise ValueError(f"Activité inattendue {compteurs}")
                return dict(
                    stimulus=stimulus,
                    entree=entree,
                    fin=apres,
                    reset=zero,
                    trace=compteurs,
                    bornes=limites,
                    journal=journal.splitlines(),
                    disque_intact=True,
                    arret_cpu=True,
                )
            finally:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                if debug:
                    debug.sock.close()
    finally:
        os.chdir(origine)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("firmware", "supports", "sortie"):
        p.add_argument(name, type=Path)
    a = p.parse_args()
    fw, s, out = map(Path.resolve, (a.firmware, a.supports, a.sortie))
    if not all(
        p.is_file() and not p.is_symlink()
        for p in (
            fw.resolve(),
            s / "standard.img",
            s / "altere.img",
            s / "actif.fd",
            s / "inactif.fd",
        )
    ):
        raise ValueError("Fichiers ordinaires requis")
    out.mkdir(parents=True, exist_ok=False)
    resultat = dict(
        ovmf=digest(fw),
        qemu=subprocess.check_output(
            ["qemu-system-x86_64", "--version"], text=True
        ).splitlines()[0],
        image=digest(s / "standard.img"),
        cas={},
    )
    with qualification(NOVA.parents[1], {}):
        for nom, mode, media, unit in [
            ("actif-dmar", "actif", "standard", True),
            ("inactif-sans-dmar", "inactif", "standard", False),
            ("inactif-tes-actif", "inactif", "standard", True),
            ("inactif-refus", "inactif", "altere", True),
            ("actif-refus-firmware", "actif", "altere", True),
        ]:
            opts = (fw, s / (mode + ".fd"), s / (media + ".img"), unit)
            if nom == "actif-refus-firmware":
                row = essai(out / nom, *opts, refus_fw=True)
            else:
                configuration = dict(
                    actif_vtd=nom == "inactif-tes-actif",
                    inconnu=nom == "inactif-sans-dmar",
                )
                positions = essai(out / (nom + "-calibration"), *opts, **configuration)
                row = essai(out / nom, *opts, bornes_prevues=positions, **configuration)
                lignes = row["journal"]
                if not any("Secure Boot " + mode in l for l in lignes):
                    raise ValueError("Mode annoncé différent du magasin")
                if nom == "inactif-sans-dmar" and not any(
                    "id=00051b36" in l and "pilote=aucun" in l for l in lignes
                ):
                    raise ValueError("Fonction témoin inconnue non identifiée")
                tes = [
                    u["gsts"] >> 31
                    for u in row["entree"]["materiel"]["unites"].values()
                ]
                if tes != ([int(configuration["actif_vtd"])] if unit else []):
                    raise ValueError("État indépendant de traduction VT-d inattendu")
                if media == "altere":
                    if not any("REFUS 6" in l for l in lignes):
                        raise ValueError("Manifeste altéré admis")
                elif not any("Inventaire termine" in l for l in lignes):
                    raise ValueError("Inventaire nominal incomplet")
            resultat["cas"][nom] = row
            (out / "mesures.json").write_text(json.dumps(resultat, indent=2) + "\n")
            print(nom, flush=True)


if __name__ == "__main__":
    main()
