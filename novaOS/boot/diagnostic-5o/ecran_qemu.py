"""Affichage local du premier PE. Banc Q35 5J, source RAM PIO, console GOP RAM et projection indépendante."""

import argparse
from hashlib import sha256
import json
from pathlib import Path
import re
import socket
import struct
import subprocess
import sys
import time

from image5o import ROOT, chargeur

sys.path.insert(0, str(ROOT / "boot/diagnostic-5m"))
from banc5m import Bail, cle_autorisee, k, pe_info, recv, sections_chargees, volume
from demarrage import directory, ram, require, public_member, STORES
from sonde import state
from observation_apic import observer, TRACE, mem
from console_ram import decrire, localiser, projeter


def run(j, folder, pe, cle, mode, cas, cal, store, temoin_pci=False):
    folder.mkdir(mode=0o700)
    with directory(folder):
        exe = k.exact(
            j / "modele5j/build/qemu-system-x86_64",
            "085ed32ca433172258c779258f435cb44388e5e6f64d9d8fd816c1fd4f27791e",
        )
        fw = k.exact(
            j / "ovmf5j-b/Build/OvmfX64/RELEASE_CLANGDWARF/FV/OVMF_CODE.fd",
            "1f767a982e7af4702235e957763ab052f29ae0bdf9f84ee08f03379ef82a70b7",
        )
        Path("variables.fd").write_bytes(store)
        volume(Path("boot.img"), pe)
        disk = Path("boot.img").read_bytes()
        require(disk.count(pe) == 1, "Origine PIO")
        bail = Bail(mode, sha256(pe).digest(), disk.index(pe), len(pe), cle)
        lease = bytes(168)
        if cal and cas != "absent":
            fields = list(cal["surface"])
            epoch = bail.epoque
            if cas == "epoque":
                epoch = bytes(16)
            if cas == "limites":
                fields[1] = 4
            if cas == "desaccord":
                fields[0] += 4096
            body = (
                b"NOVA5O-DISPLAY1!"
                + epoch
                + sha256(pe).digest()
                + struct.pack("<QQIIII", *fields)
                + bytes([int(mode == "actif"), 1, 1, 40, 0, 0, 0, 0])
            )
            lease = body + cle.sign(body)
            if cas == "signature":
                lease = lease[:-1] + bytes([lease[-1] ^ 1])
        with Path("boot.img").open("r+b") as f:
            f.seek((34 << 20) - len(bail.recu) - len(lease))
            f.write(lease + bail.recu)
        Path("epoch.bin").write_bytes(bail.ancre)
        for nom, donnees in (("recu.bin", bail.recu), ("surface.bin", lease)):
            Path(nom).write_bytes(donnees)
        canary = sha256(b"NovaOS 5O namespace synthetique").digest() * 32768
        Path("interne.raw").write_bytes(canary)
        args = k.arguments_q35(
            exe, fw, Path("boot.img").resolve(), Path("variables.fd").resolve()
        )
        ref = (
            j / "ovmf5j-b/Build/OvmfX64/RELEASE_CLANGDWARF/X64/QemuRamfbDxe.efi"
        ).read_bytes()
        # Sélecteur en lecture seule pour le GOP RAM. La projection appartient au banc.
        if temoin_pci:
            pos = args.index("edu,id=sonde") - 1
            args[pos:pos] = ["-device", "VGA,id=ecran,romfile="]
        else:
            Path("ramfb.cfg").write_bytes(bytes(28))
            args += ["-fw_cfg", "name=etc/ramfb,file=ramfb.cfg"]
        args += ["-device", "qemu-xhci,id=xhci"]
        if not cal:
            args += ["-trace", "pci_cfg_write"]
        with (
            Path("trace.log").open("xb") as trace,
            subprocess.Popen(
                args,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=trace,
                bufsize=0,
            ) as vm,
        ):
            try:
                q = k.Qmp(vm)
                (unit,) = state(q)["unites"]
                (dimm,) = q.call("query-memory-devices")
                require(
                    dimm["data"]["id"] == "labo" and dimm["data"]["addr"] >= 1 << 32,
                    "Réserve Q35",
                )
                pci = list(k.devices(q.call("query-pci")))
                require(all(d["bus"] == 0 for d in pci), "Alias ou pont inconnu")
                rids = {
                    d["qdev_id"]: 8 * d["slot"] + d["function"]
                    for d in pci
                    if d["qdev_id"]
                }
                with socket.socket(socket.AF_UNIX) as sock:
                    sock.connect("qt")
                    with sock.makefile("rwb", buffering=0) as io:
                        k.couverture(
                            k.dmar(io),
                            int(unit, 16),
                            {8 * d["slot"] + d["function"] for d in pci},
                        )
                        hw = k.Materiel(
                            q, io, int(unit, 16), dimm["data"]["addr"], rids
                        )
                        domains = hw.installer()
                        endpoints = {rids[k] for k in ("nic", "stock", "sonde")}
                        require(
                            set(domains) == endpoints and rids["xhci"] not in endpoints,
                            "Autres domaines",
                        )
                        tables = hw.tables()
                        (Path("tables.bin")).write_bytes(b"".join(tables.values()))
                        meta = pe_info(pe)
                        bail.prendre(Path("epoch.bin").read_bytes())
                        if not cal:
                            q.call("cont")
                            limit = time.monotonic() + 35
                            while (
                                "NOVA 5J transfert=" not in Path("efi.log").read_text()
                            ):
                                require(
                                    time.monotonic() < limit, "Calibration firmware"
                                )
                                time.sleep(0.04)
                            time.sleep(0.4)
                            q.call("stop")
                            data = ram(q, 0, 256 << 20)
                            rva, off, _ = meta["texte"]
                            bases = [
                                m.start() - rva
                                for m in re.finditer(
                                    re.escape(pe[off : off + 64]), data
                                )
                                if m.start() % 4096 == 0
                                and data[m.start() - rva : m.start() - rva + 2] == b"MZ"
                            ]
                            require(len(bases) == 1, "Base du PE ambiguë")
                            if temoin_pci:
                                (device,) = [
                                    d
                                    for d in k.devices(q.call("query-pci"))
                                    if d.get("qdev_id") == "ecran"
                                ]
                                bars = {r["bar"]: r for r in device["regions"]}
                                regs_pci = state(q)["pci"]
                                ident = f"0000:{device['bus']:02x}:{device['slot']:02x}.{device['function']}"
                                require(
                                    bars[0]["address"] == -1
                                    and bars[2]["address"] == -1
                                    and regs_pci[ident]["cmd"] & 6 == 0,
                                    "Garde graphique inattendue",
                                )
                                Path("barres.json").write_text(
                                    json.dumps(bars, indent=2)
                                )
                                Path("pci.json").write_text(
                                    json.dumps(regs_pci, indent=2)
                                )
                                return dict(
                                    garde_graphique=True,
                                    statut=recv(q, bases[0], meta["taille"])["statut"],
                                )
                            k.calibrer(data, bases[0] + meta["entree"], j)
                            attribution = json.loads(
                                Path("calibration.json").read_text()
                            )
                            gop = localiser(data, ref)
                            return dict(
                                base=bases[0],
                                module_gop=gop,
                                **attribution,
                                surface=decrire(q, gop, ref),
                            )
                        base = cal["base"]
                        entry, halt = base + meta["entree"], base + meta["hlt"]
                        Path("diagnostic.efi").write_bytes(pe)
                        ident = dict(
                            meta,
                            fichier=Path("diagnostic.efi"),
                            offset_entree=meta["entree"]
                            - meta["texte"][0]
                            + meta["texte"][1],
                        )
                        entree = {}

                        def inspecter(q, dbg):
                            regs_cpu = struct.unpack_from(
                                "<17Q", bytes.fromhex(dbg.command("g"))
                            )
                            rt = int.from_bytes(
                                mem(dbg, regs_cpu[3] + 0x58, 8), "little"
                            )
                            dbg.watch(int.from_bytes(mem(dbg, rt + 0x48, 8), "little"))
                            proven = decrire(q, cal["module_gop"], ref)
                            require(
                                list(proven) == list(cal["surface"])
                                and hw.tables() == tables,
                                "Bail périmé au premier RIP",
                            )
                            require(
                                all(
                                    not (
                                        proven[0] <= m["cadre"] < proven[0] + proven[1]
                                    )
                                    for d in domains.values()
                                    for m in d["mappings"]
                                ),
                                "DMA vers le framebuffer",
                            )
                            regs = state(q)["unites"][unit]
                            Path("unite.json").write_text(json.dumps(regs))
                            require(
                                k.legacy(
                                    regs, tables, {hw.page(i) for i in range(14, 18)}
                                )
                                == domains,
                                "Racine effective",
                            )
                            flags = Path("efi.log").read_text()
                            require(
                                flags.count("NOVA 5J transfert=") == 1
                                and flags.count("NOVA PIO LoadImage=0000000000000000")
                                == 1
                                and "NOVA 5J SecureBoot="
                                + f"{int(mode == 'actif'):016x}"
                                in flags,
                                "Origine ou mode",
                            )
                            for n, (rva, sz, h) in sections_chargees(pe, base).items():
                                require(
                                    sha256(ram(q, base + rva, sz)).hexdigest() == h,
                                    "Section PE " + n,
                                )
                            w, h, before = projeter(q, proven, "avant.ppm")
                            require((w, h) == proven[2:4], "Console et rectangle")
                            front = ram(q, proven[0], proven[1])
                            stats = q.stats()
                            entree.update(
                                surface=proven,
                                w=w,
                                h=h,
                                before=before,
                                front=front,
                                stats=stats,
                                regs=regs,
                                t=time.perf_counter_ns(),
                            )

                        observer(
                            q,
                            j,
                            cal,
                            diagnostic=ident,
                            verifier_entree=inspecter,
                            evenements=TRACE
                            + (
                                "apm_io_write",
                                "pci_nvme_admin_cmd",
                                "pci_nvme_io_cmd",
                                "pci_nvme_mmio_doorbell_sq",
                                "pci_nvme_mmio_doorbell_cq",
                            ),
                        )
                        ns = time.perf_counter_ns() - entree["t"]
                        proven, w, h, before, front, stats, regs = [
                            entree[n]
                            for n in (
                                "surface",
                                "w",
                                "h",
                                "before",
                                "front",
                                "stats",
                                "regs",
                            )
                        ]
                        verdict_io = json.loads(Path("verdict-apic.json").read_text())
                        require(
                            verdict_io["intervalle_admis"]
                            and verdict_io["eoi_attribues"] >= 1,
                            "Seule exception EOI 0x20",
                        )
                        rec = recv(q, base, meta["taille"])
                        afterregs = state(q)["unites"][unit]
                        require(
                            hw.tables() == tables
                            and regs == afterregs
                            and stats == q.stats()
                            and not any(stats.values()),
                            "Stockage ou VT-d modifiés",
                        )
                        delta = Path("intervalle.log").read_bytes()
                        require(
                            not re.search(
                                rb"(?:SMM:|apm_io_write|vtd_dmar_fault|pci_nvme_(?:admin_cmd|io_cmd|mmio_doorbell)|pci_cfg_write|vtd_reg_write)",
                                delta,
                            ),
                            "Commande interne ou modification PCI/VT-d",
                        )
                        _, _, after = projeter(q, proven, "apres.ppm")
                        back = ram(q, proven[0], proven[1])
                        code = rec["statut"]
                        require(
                            rec["epoque"] == bail.epoque.hex()
                            and rec["image"] == sha256(pe).hexdigest()
                            and rec["confiance"] == int(mode == "actif"),
                            "Identité terminale",
                        )
                        require(
                            code
                            == {
                                "nominal": 40,
                                "absent": 41,
                                "epoque": 41,
                                "signature": 42,
                                "limites": 44,
                                "desaccord": 44,
                            }[cas],
                            f"Verdict EFI inattendu {code}",
                        )
                        if cas == "nominal":
                            require(after != before and front != back, "Aucun rendu")
                            for y in range(h):
                                skip = 544 if y < 120 else 0
                                lo = (y * proven[4] + skip) * 4
                                hi = (y + 1) * proven[4] * 4
                                require(
                                    front[lo:hi] == back[lo:hi], "Écriture hors fenêtre"
                                )
                                require(
                                    before[(y * w + skip) * 3 : (y + 1) * w * 3]
                                    == after[(y * w + skip) * 3 : (y + 1) * w * 3],
                                    "Pixels hors fenêtre",
                                )
                            require(
                                front[h * proven[4] * 4 :] == back[h * proven[4] * 4 :],
                                "Queue du framebuffer",
                            )
                        else:
                            require(
                                before == after and front == back,
                                "Écriture malgré refus de la surface",
                            )
                        require(
                            Path("interne.raw").read_bytes() == canary, "Intégrité NVMe"
                        )
                        return dict(
                            mode=mode,
                            cas=cas,
                            verdict=rec["statut"],
                            confiance=rec["confiance"],
                            arena=rec["arena"],
                            vivants=rec["vivants"],
                            rids=rids,
                            domaines=domains,
                            base_image=base,
                            surface=proven,
                            tables_sha256=sha256(b"".join(tables.values())).hexdigest(),
                            stockage=stats,
                            eoi_attribues=verdict_io["eoi_attribues"],
                            rendu=front != back,
                            octets_fenetre=544 * 120 * 4,
                            intervalle_observe_ns=ns,
                            avant_ppm=sha256(
                                Path("avant.ppm").read_bytes()
                            ).hexdigest(),
                            apres_ppm=sha256(
                                Path("apres.ppm").read_bytes()
                            ).hexdigest(),
                            bail_sha256=sha256(lease).hexdigest(),
                            recu_sha256=sha256(bail.recu).hexdigest(),
                            premier_rip=entry,
                            arret=halt,
                        )
            finally:
                vm.terminate()
                vm.wait(timeout=10)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for n in ("build5j", "cle", "sortie"):
        p.add_argument(n, type=Path)
    p.add_argument("--mode", choices=["actif", "inactif"], default="inactif")
    p.add_argument("--magasin", type=Path)
    p.add_argument("--temoin-pci", action="store_true")
    a = p.parse_args()
    out = a.sortie.resolve()
    out.mkdir(mode=0o700)
    cle = cle_autorisee(a.cle)
    pe = chargeur()
    j = a.build5j.resolve()
    store = (
        public_member(a.mode + ".fd", STORES[a.mode])
        if a.magasin is None
        else a.magasin.read_bytes()
    )
    if a.magasin:
        from supports import comparer_magasin

        src = out / "original.fd"
        src.write_bytes(public_member("actif.fd", STORES["actif"]))
        require(a.mode == "actif", "Magasin inattendu")
        comparer_magasin(src, a.magasin, [pe_info(pe)["authenticode"]])
    with k.qualification(ROOT.parent.parent, {}):
        cal = run(j, out / "cal", pe, cle, a.mode, "cal", None, store, a.temoin_pci)
        (out / "cal.json").write_text(json.dumps(cal))
        if a.temoin_pci:
            require(cal["statut"] == 41, "Reçu écran absent")
            return
        for cas in ("nominal", "absent", "signature", "epoque", "desaccord", "limites"):
            result = run(j, out / cas, pe, cle, a.mode, cas, cal, store)
            (out / (cas + ".json")).write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
