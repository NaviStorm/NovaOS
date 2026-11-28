"""Capture Q35 mono-vCPU. Les points d’arrêt observent la première instruction du PE."""

import argparse
from hashlib import sha256
import importlib.util
import json
import os
from pathlib import Path
import re
import socket
import struct
import subprocess
import sys
import time
from supports import (
    check,
    digest,
    sauver,
    pe_info,
    comparer_magasin,
    MODES,
    NOVA,
    volume,
    replace_once,
)
from transfert import Bail, ORIGINE, TAILLE, cle_autorisee
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from image import sections_chargees
from pyfatfs.PyFatFS import PyFatFS
from virt.firmware.varstore.edk2 import Edk2VarStore

K = NOVA / "boot/diagnostic-5k"
sys.path.insert(0, str(K))
spec = importlib.util.spec_from_file_location("retrait_5k", K / "banc.py")
k = importlib.util.module_from_spec(spec)
spec.loader.exec_module(k)
from observation_apic import Debug, observer, mem, TRACE

CAS = (
    "valide",
    "altere",
    "signataire",
    "reserve",
    "absent",
    "perime",
    "reutilise",
    "manifeste",
    "paquet",
    "substitution",
)
ATTENDU = {
    "valide": 0,
    "altere": 25,
    "signataire": 25,
    "reserve": 26,
    "absent": 23,
    "perime": 24,
    "manifeste": 6,
    "paquet": 18,
}
MARQUE = b"NOVA5M-RAM-FINAL"


def origine(p, images, nom):
    with PyFatFS(str(p), read_only=True) as fs:
        b = fs.readbytes("EFI/BOOT/BOOTX64.EFI")
    check(pe_info(b) == images[nom], "Image hors autorisation")
    data = p.read_bytes()
    check(data.count(b) == 1, "Source de chargement non contiguë ou ambiguë")
    return b, data.index(b)


def preparer(out, s, mode, cas, precedent, cle):
    images = json.loads((s / "images.json").read_text())
    nom = cas if cas in ("manifeste", "paquet") else "valide"
    media = s / (nom + ".img")
    pe, offset = origine(media, images, nom)
    # Le profil négatif offre un reçu et un engagement cohérents avec un autre
    # signataire pour exercer la racine épinglée à l'intérieur du PE.
    signataire = Ed25519PrivateKey.generate() if cas == "signataire" else cle
    bail = Bail(mode, sha256(pe).digest(), offset, len(pe), signataire)
    if cas == "reserve":
        # Vecteur signé volontairement non canonique, rejeté par le schéma v1.
        bail.corps = bail.corps[:28] + bytes((1, 0, 0, 0)) + bail.corps[32:]
        bail.recu = bail.corps + cle.sign(bail.corps)
        bail.ancre = sha256(ORIGINE + bail.corps).digest()
    recu, ancre = bail.recu, bail.ancre
    if cas == "altere":
        recu = recu[:-1] + bytes([recu[-1] ^ 1])
    if cas == "absent":
        recu = bytes(TAILLE)
    if cas in ("perime", "reutilise"):
        recu = (precedent / "transfert.bin").read_bytes()
        if cas == "reutilise":
            ancre = (precedent / "epoch.bin").read_bytes()
    if cas == "substitution":
        # Le support essayé contient une autre image valide, autorisée pour son scénario propre.
        media = s / "paquet.img"
    if cas == "refus-db":
        media = out / "hors-db.img"
        bad = replace_once(
            pe, b"NOVA5E-CFG!\x01\x05\x01\x00\x03", b"NOVA5E-CFG!\x01\x04\x01\x00\x03"
        )
        volume(media, bad)
        (out / "hors-db.efi").write_bytes(bad)
    fat = out / "support.img"
    with fat.open("xb") as f:
        f.write(media.read_bytes())
        f.seek((34 << 20) - TAILLE)
        f.write(recu)
    (out / "transfert.bin").write_bytes(recu)
    (out / "epoch.bin").write_bytes(ancre)
    return nom, bail, images[nom], fat


def recv(q, base, taille):
    q.call("pmemsave", val=base, size=taille, filename="prive.bin")
    p = Path("prive.bin")
    try:
        data = p.read_bytes()
        candidats = [
            i.start()
            for i in re.finditer(MARQUE, data)
            if struct.unpack_from("<Q", data, i.start() + 16)[0] == base + i.start()
        ]
        (pos,) = candidats
        raw = data[pos : pos + 104]
        marque, adr, status, sb, arena, live, epoch, h = struct.unpack(
            "<16s5Q16s32s", raw
        )
        check(marque == MARQUE and live == 0, "Arrêt ou allocations")
        Path("sortie.bin").write_bytes(raw)
        return dict(
            adresse=adr,
            statut=status,
            confiance=sb,
            arena=arena,
            vivants=live,
            epoque=epoch.hex(),
            image=h.hex(),
        )
    finally:
        p.unlink(missing_ok=True)


def essai(j, s, out, mode, cas, cle, cal=None, precedent=None):
    out.mkdir()
    os.chdir(out)
    exe = k.exact(
        j / "modele5j/build/qemu-system-x86_64",
        "085ed32ca433172258c779258f435cb44388e5e6f64d9d8fd816c1fd4f27791e",
    )
    fw = k.exact(
        j / "ovmf5j-b/Build/OvmfX64/RELEASE_CLANGDWARF/FV/OVMF_CODE.fd",
        "1f767a982e7af4702235e957763ab052f29ae0bdf9f84ee08f03379ef82a70b7",
    )
    var = s / (mode + ".fd")
    v = Edk2VarStore(str(var)).get_varlist()
    check(
        v["SecureBootEnable"].data == bytes([int(mode == "actif")])
        and "AuditMode" not in v
        and "PK" in v,
        "Politique persistante",
    )
    original = j / "qualification5j" / (MODES[mode][0] + ".fd")
    k.exact(original, MODES[mode][1])
    if mode == "actif":
        diff = json.loads((s / "magasin.json").read_text())
        images = json.loads((s / "images.json").read_text())
        check(
            comparer_magasin(
                original,
                var,
                [images[n]["authenticode"] for n in ("valide", "manifeste", "paquet")],
            )
            == diff,
            "Admission db",
        )
    else:
        k.exact(var, MODES[mode][1])
    check(
        (s / "images.json").read_bytes()
        == Path(__file__).with_name("images-laboratoire.json").read_bytes(),
        "Catalogue de campagne non autorisé",
    )
    nom, bail, profil, fat = preparer(out, s, mode, cas, precedent, cle)
    disk = Path("interne.raw")
    disk.write_bytes(sha256(b"NovaOS/5M/namespace").digest() * 131072)
    initial = digest(disk.read_bytes())
    log = Path("efi.log")
    result = dict(
        mode=mode,
        cas=cas,
        epoque=bail.epoque.hex(),
        image_attendue=profil["sha256"],
        magasin=digest(var.read_bytes()),
        namespace_avant=initial,
    )
    args = k.arguments_q35(exe, fw, fat, var)
    with (
        open("trace.log", "xb") as trace,
        subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=trace, bufsize=0
        ) as vm,
    ):
        try:
            q = k.Qmp(vm)
            (dimm,) = q.call("query-memory-devices")
            check(
                dimm["type"] == "nvdimm"
                and dimm["data"]["id"] == "labo"
                and dimm["data"]["addr"] >= 1 << 32,
                "Réserve",
            )
            (unit,) = k.state(q)["unites"]
            pci = list(k.devices(q.call("query-pci")))
            check(all(d["bus"] == 0 for d in pci), "Alias inconnu")
            rids = {
                d["qdev_id"]: d["slot"] * 8 + d["function"] for d in pci if d["qdev_id"]
            }
            with socket.socket(socket.AF_UNIX) as sock:
                sock.connect("qt")
                with sock.makefile("rwb", buffering=0) as io:
                    k.couverture(
                        k.dmar(io),
                        int(unit, 16),
                        {d["slot"] * 8 + d["function"] for d in pci},
                    )
                    hw = k.Materiel(q, io, int(unit, 16), dimm["data"]["addr"], rids)
                    result["domaines"] = hw.installer()
                    before = hw.tables()
                    Path("tables-avant.bin").write_bytes(b"".join(before.values()))

                    def admission():
                        b, _ = origine(
                            fat, json.loads((s / "images.json").read_text()), nom
                        )
                        check(digest(b) == profil["sha256"], "PE substitué")
                        bail.prendre(Path("epoch.bin").read_bytes())

                    if cas in ("substitution", "reutilise"):
                        try:
                            admission()
                        except ValueError as e:
                            result.update(
                                refus_avant_chargement=str(e), cpu_libere=False
                            )
                        else:
                            raise ValueError("Prise de contrôle indue")
                    elif cas == "refus-db":
                        check(
                            mode == "actif" and cal, "Témoin db réservé au mode actif"
                        )
                        # Si LoadImage admettait cette image, le point d'arrêt retient la première instruction.
                        dbg = Debug()
                        dbg.watch(cal["entree"])
                        q.call("cont")
                        limite = time.monotonic() + 14
                        while "NOVA PIO terminal=" not in log.read_text():
                            check(time.monotonic() < limite, "Refus firmware absent")
                            time.sleep(0.02)
                        q.call("stop")
                        fwlog = log.read_text()
                        check(
                            "NOVA PIO LoadImage=800000000000000f" in fwlog
                            and "NOVA PIO terminal=800000000000000f" in fwlog
                            and "NOVA 5J transfert=" not in fwlog,
                            "EFI_ACCESS_DENIED attendu",
                        )
                        result.update(
                            refus_firmware="EFI_ACCESS_DENIED",
                            cpu_libere=True,
                            premier_efi=False,
                            image_refusee=pe_info(Path("hors-db.efi").read_bytes()),
                        )
                    elif cal is None:
                        admission()
                        q.call("cont")
                        deadline = time.monotonic() + 14
                        marker = "NOVA 5J transfert="
                        while marker not in log.read_text():
                            check(
                                time.monotonic() < deadline, "Calibration EFI absente"
                            )
                            time.sleep(0.025)
                        time.sleep(0.4)
                        q.call("stop")
                        rec = recv(q, 0, 256 << 20)
                        q.call(
                            "pmemsave", val=0, size=256 << 20, filename="prive-cal.bin"
                        )
                        p = Path("prive-cal.bin")
                        try:
                            k.calibrer(p.read_bytes(), rec["adresse"], j)
                        finally:
                            p.unlink(missing_ok=True)
                        result["sortie"] = rec
                    else:
                        admission()
                        diag = dict(
                            profil,
                            fichier=s / (nom + ".efi"),
                            offset_entree=profil["entree"]
                            - profil["texte"][0]
                            + profil["texte"][1],
                        )

                        def entree(q, dbg):
                            entry = cal["image"] + profil["entree"]
                            check(
                                cal["entree"] == entry and dbg.pc() == entry,
                                "Premier RIP",
                            )
                            flags = log.read_text()
                            check(
                                flags.count("NOVA 5J transfert=") == 1
                                and flags.count("NOVA PIO LoadImage=0000000000000000")
                                == 1
                                and flags.count(
                                    "NOVA 5J SecureBoot="
                                    + f"{int(mode == 'actif'):016x}"
                                )
                                == 1,
                                "Mode firmware ou démarrage ambigus",
                            )
                            regs = struct.unpack_from(
                                "<17Q", bytes.fromhex(dbg.command("g"))
                            )
                            rt = int.from_bytes(mem(dbg, regs[3] + 0x58, 8), "little")
                            variable = int.from_bytes(mem(dbg, rt + 0x48, 8), "little")
                            check(variable > 0, "Adresse de GetVariable")
                            dbg.watch(variable)
                            sections = sections_chargees(
                                diag["fichier"].read_bytes(), cal["image"]
                            )
                            for section, (rva, sz, attendu) in sections.items():
                                p = Path("section.bin")
                                q.call(
                                    "pmemsave",
                                    val=cal["image"] + rva,
                                    size=sz,
                                    filename=str(p),
                                )
                                try:
                                    check(
                                        digest(p.read_bytes()) == attendu,
                                        "Section chargée divergente " + section,
                                    )
                                finally:
                                    p.unlink(missing_ok=True)
                            result["entree"] = dict(
                                rip=entry,
                                base=cal["image"],
                                get_variable=variable,
                                sections=sections,
                                octets=mem(dbg, entry, 16).hex(),
                            )
                            result["pci_avant"] = k.state(q)
                            check(
                                before == hw.tables() and not any(q.stats().values()),
                                "Protection ou E/S avant diagnostic",
                            )

                        observer(
                            q,
                            j,
                            cal,
                            diagnostic=diag,
                            verifier_entree=entree,
                            evenements=TRACE + ("apm_io_write", "pci_nvme_io_cmd"),
                        )
                        inter = Path("intervalle.log").read_bytes()
                        red = json.loads(Path("verdict-apic.json").read_text())
                        check(
                            red["intervalle_admis"]
                            and red["eoi_attribues"] >= 1
                            and not any(
                                x in inter
                                for x in (
                                    b"SMM:",
                                    b"apm_io_write",
                                    b"pci_nvme_io_cmd",
                                    b"vtd_dmar_fault",
                                )
                            ),
                            "Écriture ou SMM inattendus",
                        )
                        rec = recv(q, cal["image"], profil["taille"])
                        check(
                            rec["statut"] == ATTENDU[cas]
                            and rec["confiance"]
                            == (
                                int(mode == "actif")
                                if cas in ("valide", "manifeste", "paquet")
                                else 2
                            ),
                            "Résultat terminal inattendu " + str(rec),
                        )
                        if rec["confiance"] != 2:
                            check(
                                rec["image"] == profil["sha256"]
                                and rec["epoque"] == bail.epoque.hex(),
                                "Identité terminale",
                            )
                        result.update(
                            sortie=rec,
                            pci_apres=k.state(q),
                            eoi=red["eoi_attribues"],
                            cpu_libere=True,
                        )
                        check(
                            result["pci_avant"] == result["pci_apres"],
                            "PCI/VT-d modifié",
                        )
                        if cas == "valide":
                            try:
                                bail.prendre(bail.ancre)
                            except ValueError:
                                result["seconde_admission_meme_vm"] = "refusee"
                            else:
                                raise ValueError("Seconde admission autorisée")

                    after = hw.tables()
                    Path("tables-apres.bin").write_bytes(b"".join(after.values()))
                    check(
                        before == after and not any(q.stats().values()),
                        "Tables ou stockage modifiés",
                    )
            q.call("quit")
            vm.wait(timeout=5)
        finally:
            if vm.poll() is None:
                vm.kill()
                vm.wait()
    result["namespace_apres"] = digest(disk.read_bytes())
    check(result["namespace_apres"] == initial, "Namespace modifié")
    sauver(Path("resultat.json"), result)
    return result


def main():
    p = argparse.ArgumentParser()
    for n in ("build5j", "supports", "sortie"):
        p.add_argument(n, type=Path)
    p.add_argument("--modes", nargs="+", choices=MODES, default=list(MODES))
    p.add_argument(
        "--cle-recu",
        type=Path,
        required=True,
        help="Chemin privé de la clé de reçu de laboratoire",
    )
    a = p.parse_args()
    cle = cle_autorisee(a.cle_recu)
    j, s, out = (f.resolve() for f in (a.build5j, a.supports, a.sortie))
    out.mkdir(parents=True, exist_ok=False)
    with k.qualification(NOVA.parents[1], {}):
        for mode in a.modes:
            pilot = out / ("cal-" + mode)
            r = essai(j, s, pilot, mode, "valide", cle)
            print("Calibration", mode, r["sortie"], flush=True)
            check(r["sortie"]["statut"] == 0, "Calibration refusée")
            cal = json.loads((pilot / "calibration.json").read_text())
            for cas in CAS + (("refus-db",) if mode == "actif" else ()):
                r = essai(
                    j,
                    s,
                    out / (mode + "-" + cas),
                    mode,
                    cas,
                    cle,
                    cal,
                    out / (mode + "-valide"),
                )
                print(
                    mode,
                    cas,
                    r.get("sortie", {}).get("statut"),
                    r.get("eoi"),
                    flush=True,
                )


if __name__ == "__main__":
    main()
