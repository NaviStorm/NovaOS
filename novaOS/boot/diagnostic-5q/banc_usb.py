"""Chargeur USB protégé, exclusivement sur le modèle Q35/OVMF de laboratoire."""

import argparse
from contextlib import ExitStack
import gzip
import json
from pathlib import Path
import re
import socket
import struct
import subprocess
import sys
import time

from politique import FIRMWARE, PAGE, Registre, canon, empreinte, exiger, source_usb

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5p"))
from observation import state, devices, activite
from origine import chemin
from temoin_usb import partition
from image5o import PREUVES, chargeur, creer, lecteur, MIO
from demarrage import directory, ram, count, STORES, public_member
from verifier5n import geometry
from image import sections_chargees, pe_info

sys.path.insert(0, str(NOVA / "boot/diagnostic-5m"))
from banc5m import k, recv, Bail, cle_autorisee
from observation_apic import observer, TRACE
from console_ram import decrire, localiser, projeter


def save(name, obj):
    Path(name).write_text(json.dumps(obj, indent=2, sort_keys=True) + "\n")


def qbytes(hw, address, data):
    hw.demande(f"write {address} {len(data)} 0x" + data.hex())


def pages(hw):
    return {
        hw.page(i): hw.octets(hw.page(i), PAGE) for i in (*range(14), 20, 21, 22, 23)
    }


def live(hw, regs, baseline, ledger):
    raw = pages(hw)
    permitted = {hw.page(i) for i in range(14, 18)}
    maps = ledger.feuilles()
    decoded = k.legacy(regs, raw, permitted | {x["cadre"] for x in maps})
    expected = dict(baseline)
    expected[ledger.rid] = dict(did=4, mappings=maps)
    exiger(decoded == expected, "Tables VT-d différentes des baux")
    return raw


def support_intact(media, attendu):
    exiger(empreinte(media.read_bytes()) == attendu, "Image USB modifiée")
    return attendu


def save_tables(name, raw):
    Path(name).write_bytes(b"".join(raw[k] for k in sorted(raw)))


def initial(hw, rid):
    old = hw.installer()
    exiger(
        rid not in old and not hw.registre(8) & 128,
        "Contexte existant ou cache négatif",
    )
    exiger(
        all(not any(hw.octets(hw.page(i), PAGE)) for i in range(20, 25)),
        "Réserve occupée",
    )
    ctx = hw.page(1) + 16 * rid
    exiger(hw.lire(ctx) == 0 and hw.lire(ctx + 8) == 0, "Contexte déjà publié")
    for i in range(20, 23):
        hw.ecrire(hw.page(i), hw.page(i + 1) | 3)
    hw.ecrire(ctx + 8, 4 << 8 | 2)
    hw.ecrire(ctx, hw.page(20) | 1)
    return old


def publish(hw, ledger, seq, role, length):
    op = ledger.proposer(seq, role, length)
    for i in range(op["pages"]):
        leaf = hw.page(23) + (op["iova"] // PAGE + i) * 8
        exiger(hw.lire(leaf) == 0, "Feuille préexistante")
    for i in range(op["pages"]):
        leaf = hw.page(23) + (op["iova"] // PAGE + i) * 8
        value = (op["gpa"] + i * PAGE) | op["droits"]
        hw.ecrire(leaf, value)
        exiger(hw.lire(leaf) == value, "Publication de feuille incertaine")
    ledger.confirmer(op)
    return op


def arm_words(hw, base):
    return struct.unpack("<32Q", hw.octets(base, 256))


def trace_marks():
    return Path("trace.log").stat().st_size


def media_path(q):
    return (
        q.call("human-monitor-command", **{"command-line": "info usb"})
        .strip()
        .splitlines()
    )


def arguments_usb(exe, fw, src, media, case, plugin, files, fds):
    args = k.arguments_q35(exe, fw, Path("inutilise"), Path("variables.fd").resolve())
    while "-fw_cfg" in args:
        p = args.index("-fw_cfg")
        del args[p : p + 2]
    args += [
        "-plugin",
        str(plugin),
        "-fw_cfg",
        "name=etc/ramfb,file=ramfb.cfg",
        "-fw_cfg",
        "name=opt/novaos/5q-locator,file=locator.bin",
        "-device",
        "qemu-xhci,id=xhci",
        "-add-fd",
        f"fd={src.fileno()},set=1,opaque=usb5q",
        "-drive",
        "if=none,id=usb5n,format=raw,readonly=on,file.driver=file,file.filename=/dev/fdset/1,file.locking=off"
        + (",throttling.bps-read=4194304" if case == "vol" else ""),
        "-device",
        "usb-storage,id=cle,bus=xhci.0,port=1,drive=usb5n",
    ]
    if case in ("clone", "reconnexion"):
        clone = files.enter_context(lecteur(media.parent / "clone.img"))
        fds.append(clone.fileno())
        args += [
            "-add-fd",
            f"fd={clone.fileno()},set=2,opaque=clone5q",
            "-drive",
            "if=none,id=clone5n,format=raw,readonly=on,file.driver=file,file.filename=/dev/fdset/2,file.locking=off",
        ]
        if case == "clone":
            args += [
                "-device",
                "usb-storage,id=clone,bus=xhci.0,port=2,drive=clone5n",
            ]
    for ev in (
        "usb_xhci_xfer_start",
        "usb_xhci_slot_address",
        "usb_xhci_ep_enable",
        "usb_msd_cmd_submit",
        "usb_msd_cmd_complete",
        "usb_xhci_xfer_async",
        "usb_xhci_xfer_success",
        "usb_xhci_xfer_error",
        "vtd_iotlb_page_update",
        "vtd_reg_write",
        "pci_nvme_admin_cmd",
        "pci_nvme_io_cmd",
        "pci_nvme_mmio_doorbell_sq",
        "pci_nvme_mmio_doorbell_cq",
    ):
        args += ["-trace", ev]
    return args


def observer_vol(hw, box, ledger, info):
    pair = arm_words(hw, box)[8:10]
    if pair[0] > pair[1]:
        prefix = Path("trace.log").read_bytes()
        if b"usb_xhci_xfer_async " in prefix[-6000:]:
            # Un stop QMP draine certains backends. Échantillonner la VM en marche.
            prefix = prefix[: prefix.rfind(b"\n") + 1]
            obs = activite(
                l.decode() for l in prefix.splitlines() if l.startswith(b"usb_xhci_")
            )
            if (
                not obs["anomalies"]
                and any(e > 1 and busy for _, e, busy in obs["vols"])
                and arm_words(hw, box)[8:10] == pair
                and b"usb_xhci_xfer_"
                not in Path("trace.log").read_bytes()[len(prefix) :]
            ):
                try:
                    ledger.clore(*pair, 1)
                except ValueError:
                    pass
                else:
                    raise ValueError("Restitution prématurée")
                info["en_vol"] = dict(
                    position=len(prefix),
                    soumis=pair[0],
                    clos=pair[1],
                    requetes=obs["vols"],
                    decision="refus de restitution",
                )


def perdre_incarnation(q, hw, box, media, info, ledger):
    q.call("device_del", id="cle")
    info["deconnecte"] = media_path(q)
    exiger(
        not info["deconnecte"],
        "Le premier support reste attaché",
    )
    q.call(
        "device_add",
        driver="usb-storage",
        id="autre",
        bus="xhci.0",
        port="1",
        drive="clone5n",
    )
    exiger(
        any("ID: autre" in l for l in media_path(q)),
        "Reconnexion manquante",
    )
    info["reconnecte"] = media_path(q)
    info["clone_sha256"] = support_intact(media.parent / "clone.img", info["image"])
    hw.ecrire(box + 16, 6)
    save(
        "bilan.json",
        dict(
            info,
            decision="quarantaine-reconnexion",
            prets=ledger.loans,
            apres=count(q),
        ),
    )


def run(folder, exe, fw, j, media, key, mode, case, plugin):
    folder.mkdir(mode=0o700)
    with directory(folder), ExitStack() as files:
        src = files.enter_context(lecteur(media))
        fds = [src.fileno()]
        expected_part = partition(src)
        src.seek(0)
        disk = src.read()
        pos, size = geometry(disk)["NOVAOS"]
        esp = disk[pos : pos + size]
        pe = chargeur()
        exiger(esp.count(pe) == 1, "PE USB absent ou fragmenté dans le vecteur")
        offset = esp.index(pe)
        bail = Bail(mode, bytes.fromhex(empreinte(pe)), offset, len(pe), key)
        epoch = bail.epoque.hex()
        store = (
            gzip.decompress((PREUVES / "magasin-ovmf-test.fd.gz").read_bytes())
            if mode == "actif"
            else public_member("inactif.fd", STORES["inactif"])
        )
        exiger(
            empreinte(store)
            == (
                "bf41a49e08223f8e0a0192371d8fb79602faf56e4c0a58a5ad79624a54f2ed8f"
                if mode == "actif"
                else STORES["inactif"]
            ),
            "Magasin inattendu",
        )
        Path("variables.fd").write_bytes(store)
        canary = bytes(range(256)) * 16384
        Path("interne.raw").write_bytes(canary)
        # Géométrie choisie par le banc et vérifiée dans QMP avant toute instruction.
        reserve = 1 << 32
        box = reserve + 24 * PAGE
        Path("locator.bin").write_bytes(struct.pack("<Q", box))
        Path("ramfb.cfg").write_bytes(bytes(28))
        args = arguments_usb(exe, fw, src, media, case, plugin, files, fds)
        with (
            Path("trace.log").open("xb") as log,
            subprocess.Popen(
                args,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=log,
                pass_fds=fds,
                bufsize=0,
            ) as vm,
        ):
            try:
                q = k.Qmp(vm)
                pci = list(devices(q.call("query-pci")))
                exiger(all(d["bus"] == 0 for d in pci), "Topologie PCI inconnue")
                ids = {
                    d["qdev_id"]: 8 * d["slot"] + d["function"]
                    for d in pci
                    if d["qdev_id"]
                }
                (ctl,) = [d for d in pci if d["class_info"]["class"] == 0xC03]
                rid = ids[ctl["qdev_id"]]
                exiger(rid == ids["xhci"], "RID USB contradictoire")
                (dimm,) = q.call("query-memory-devices")
                exiger(
                    dimm["data"]["addr"] == reserve
                    and dimm["data"]["id"] == "labo"
                    and dimm["data"]["size"] == 2 * MIO,
                    "Réserve du banc",
                )
                (unit,) = state(q)["unites"]
                ledger = Registre(reserve, rid, epoch)
                with socket.socket(socket.AF_UNIX) as sock:
                    sock.connect("qt")
                    with sock.makefile("rwb", buffering=0) as ch:
                        k.couverture(
                            k.dmar(ch),
                            int(unit, 16),
                            {d["slot"] * 8 + d["function"] for d in pci},
                        )
                        hw = k.Materiel(q, ch, int(unit, 16), reserve, ids)
                        t = time.monotonic_ns()
                        old = initial(hw, rid)
                        regs = state(q)["unites"][unit]
                        raw = live(hw, regs, old, ledger)
                        save_tables("initial.bin", raw)
                        setup = time.monotonic_ns() - t
                        counters = count(q)
                        before = media_path(q)
                        exiger(
                            before and all("QEMU USB MSD" in s for s in before),
                            "Support non attribué",
                        )
                        info = dict(
                            mode=mode,
                            cas=case,
                            publications=[],
                            epoque=epoch,
                            rids=ids,
                            unite=regs,
                            domaines_initiaux=old,
                            reserve=reserve,
                            installation_ns=setup,
                            avant=counters,
                            origine_qmp=before,
                            image=empreinte(disk),
                            pe=empreinte(pe),
                            firmware=empreinte(fw.read_bytes()),
                            magasin=empreinte(store),
                        )
                        if case == "clone":
                            exiger(len(before) == 2, "Clone absent")
                            info["clone_sha256"] = support_intact(
                                media.parent / "clone.img", info["image"]
                            )
                            save(
                                "bilan.json",
                                dict(
                                    info,
                                    decision="refus-clone",
                                    pages_usb=0,
                                    apres=count(q),
                                ),
                            )
                            return
                        exiger(
                            len(before) == 1 and "Port 1," in before[0],
                            "Chemin USB non unique",
                        )
                        hw.ecrire(box, 0x3542514F564F4E)
                        hw.ecrire(box + 8, rid)
                        qbytes(hw, box + 48 * 8, bytes.fromhex(expected_part[4]))
                        qbytes(hw, box + 52 * 8, bytes.fromhex(epoch))
                        hw.ecrire(box + 11 * 8, 1)
                        info["debut_vcpu"] = trace_marks()
                        q.call("cont")
                        start = time.monotonic_ns()
                        limit = time.monotonic() + 85
                        prepare = None
                        grant_ns = 0
                        while time.monotonic() < limit:
                            status = hw.lire(box + 16)
                            if status == 1:
                                words = arm_words(hw, box)
                                if case == "invalide":
                                    try:
                                        ledger.proposer(words[3], 0xFFFF, words[5])
                                    except ValueError:
                                        pass
                                    else:
                                        raise ValueError("Rôle invalide accepté")
                                    hw.ecrire(box + 16, 3)
                                    refus = time.monotonic() + 5
                                    while hw.lire(box + 16) != 6:
                                        exiger(
                                            time.monotonic() < refus,
                                            "Refus xHCI absent",
                                        )
                                        time.sleep(0.002)
                                    q.call("stop")
                                    info["refus_firmware"] = hw.lire(box + 10 * 8)
                                    exiger(
                                        info["refus_firmware"] == 2, "Cause arrêt xHCI"
                                    )
                                    exiger(
                                        not ledger.loans
                                        and live(hw, regs, old, ledger) == raw,
                                        "Table modifiée sur refus",
                                    )
                                    save(
                                        "bilan.json",
                                        dict(
                                            info,
                                            decision="refus-mapping",
                                            pages_usb=0,
                                            apres=count(q),
                                        ),
                                    )
                                    return
                                begin = time.monotonic_ns()
                                op = publish(hw, ledger, words[3], words[4], words[5])
                                hw.ecrire(box + 48, op["gpa"])
                                hw.ecrire(box + 56, op["iova"])
                                info["publications"].append(trace_marks())
                                hw.ecrire(box + 16, 2)
                                grant_ns += time.monotonic_ns() - begin
                            elif status == 7:
                                q.call("stop")
                                w = arm_words(hw, box)
                                exiger(
                                    prepare is None
                                    and w[27] == 1
                                    and w[13] == size
                                    and w[12] + size + 16384 < reserve
                                    and ram(q, w[12], size) == esp
                                    and w[8] == w[9]
                                    and not w[10],
                                    "ESP non attribuée avant LoadImage",
                                )
                                exiger(
                                    count(q)["interne"] == counters["interne"]
                                    and count(q)["usb5n"]["rd_bytes"] >= size,
                                    "Lecture USB ou stockage",
                                )
                                prepare = live(
                                    hw, state(q)["unites"][unit], old, ledger
                                )
                                save_tables("avant-load.bin", prepare)
                                info["avant_load"] = dict(
                                    position=trace_marks(),
                                    base=w[12],
                                    octets=size,
                                    sha256=empreinte(esp),
                                    compteurs=count(q),
                                    duree_ns=time.monotonic_ns() - start,
                                )
                                if case == "reconnexion":
                                    perdre_incarnation(q, hw, box, media, info, ledger)
                                    return
                                hw.ecrire(box + 16, 8)
                                q.call("cont")
                            elif status == 4:
                                q.call("stop")
                                break
                            elif status == 6:
                                raise ValueError(
                                    "Refus du firmware " + str(arm_words(hw, box))
                                )
                            else:
                                if case == "vol" and "en_vol" not in info:
                                    observer_vol(hw, box, ledger, info)
                                time.sleep(0.002)
                        else:
                            raise TimeoutError(
                                "Transfert firmware " + str(arm_words(hw, box))
                            )
                        w = arm_words(hw, box)
                        exiger(
                            prepare
                            and w[23] == int(mode == "actif")
                            and w[27] == 2
                            and w[10] == 0,
                            "État firmware avant StartImage",
                        )
                        # SET_ADDRESS attribue une adresse USB pendant l'énumération. Le lien QOM, le port,
                        # le descripteur bloc ouvert et l'objet gardé par le banc identifient l'incarnation.
                        devices_end = media_path(q)
                        exiger(
                            [s.partition(", Port ")[2] for s in devices_end]
                            == [s.partition(", Port ")[2] for s in before]
                            and q.call(
                                "qom-get",
                                path="/machine/peripheral/cle",
                                property="parent_bus",
                            ).endswith("/xhci/xhci.0"),
                            "Incarnation ou bus USB modifié",
                        )
                        (disk_link,) = [
                            r
                            for r in q.call("query-block")
                            if r.get("device") == "usb5n"
                        ]
                        exiger(
                            disk_link["inserted"]["ro"]
                            and disk_link["inserted"]["file"] == "/dev/fdset/1",
                            "Backend USB substitué",
                        )
                        info["usb_attribue"] = devices_end
                        exiger(
                            live(hw, state(q)["unites"][unit], old, ledger) == prepare,
                            "Traductions modifiées avant StartImage",
                        )
                        if case == "vol":
                            exiger("en_vol" in info, "Requête en vol non observée")
                        ledger.clore(w[8], w[9], 1)
                        rawpath = hw.octets(box + 32 * 8, w[16])
                        nodes = chemin(lambda a, n: rawpath[a : a + n], 0)
                        port = source_usb(nodes, rid, expected_part)
                        exiger(port == 1, "Désaccord port physique émulé")
                        meta = pe_info(pe)
                        base = w[24]
                        entry = base + meta["entree"]
                        exiger(
                            w[25] == meta["taille"] and base + w[25] < reserve,
                            "Étendue du PE",
                        )
                        sections = {
                            n: [r, s, empreinte(ram(q, base + r, s))]
                            for n, (r, s, _) in sections_chargees(pe, base).items()
                        }
                        exiger(
                            sections
                            == {
                                n: list(v)
                                for n, v in sections_chargees(pe, base).items()
                            },
                            "Substitution du PE",
                        )
                        reference = (
                            j
                            / "ovmf5j-b/Build/OvmfX64/RELEASE_CLANGDWARF/X64/QemuRamfbDxe.efi"
                        ).read_bytes()
                        low = ram(q, 0, 256 * MIO)
                        gop = localiser(low, reference)
                        surface = decrire(q, gop, reference)
                        exiger(
                            list(surface) == list(w[17:23])
                            and surface[0] + surface[1] < reserve,
                            "Surface non attribuable",
                        )
                        k.calibrer(low, entry, j)
                        cal = json.loads(Path("calibration.json").read_text())
                        del low
                        contract = dict(
                            version=1,
                            epoque=epoch,
                            incarnation=1,
                            source=nodes,
                            port=port,
                            route=[],
                            lun=0,
                            rid=rid,
                            domaines=ids,
                            ressources=ledger.loans,
                            transactions=dict(soumises=w[8], achevees=w[9]),
                            surface=list(surface),
                            secure_boot=mode,
                            pe=empreinte(pe),
                            esp=empreinte(esp),
                            image=empreinte(disk),
                            sections=sections,
                            tables=empreinte(b"".join(prepare.values())),
                            proprietaire="firmware Q35 de cette époque",
                            reste="réemploi et export natif interdits",
                        )
                        facts = [
                            rid,
                            1,
                            w[23],
                            port,
                            w[8],
                            w[9],
                            base,
                            w[25],
                            *surface,
                            offset,
                            len(pe),
                        ]
                        if case == "contradiction":
                            facts[0] ^= 8
                        signed_epoch = (
                            bytes.fromhex(epoch)
                            if case != "perime"
                            else bytes(b ^ 255 for b in bytes.fromhex(epoch))
                        )
                        payload = canon(contract)
                        body = (
                            b"NOVA5Q-USB-V1!!!"
                            + signed_epoch
                            + bytes.fromhex(contract["esp"])
                            + bytes.fromhex(contract["pe"])
                            + bytes.fromhex(empreinte(rawpath))
                            + bytes.fromhex(empreinte(payload))
                            + struct.pack("<16Q", *facts)
                            + payload
                        )
                        signature = key.sign(body)
                        key.public_key().verify(signature, body)
                        envelope = body + signature
                        if case == "signature":
                            envelope = envelope[:-1] + bytes([envelope[-1] ^ 1])
                        exiger(len(envelope) <= 12000, "Contrat hors du suffixe")
                        qbytes(hw, w[12] + size, envelope)
                        hw.ecrire(box + 14 * 8, offset)
                        hw.ecrire(box + 15 * 8, len(pe))
                        hw.ecrire(box + 26 * 8, len(envelope))
                        Path("origine.bin").write_bytes(rawpath)
                        Path("transfert.bin").write_bytes(envelope)
                        save("contrat.json", contract)
                        Path("contrat.sig").write_bytes(signature)
                        bail.prendre(bail.ancre)
                        # Même époque pour le reçu historique et l'attestation complète.
                        exiger(bail.epoque != bytes(16), "Époque vide")
                        body = (
                            b"NOVA5O-DISPLAY1!"
                            + bail.epoque
                            + bytes.fromhex(contract["pe"])
                            + struct.pack("<QQIIII", *surface)
                            + bytes([int(mode == "actif"), 1, 1, 40, 0, 0, 0, 0])
                        )
                        lease = body + key.sign(body)
                        qbytes(hw, box + 128 * 8, bail.ancre + lease + bail.recu)
                        Path("recu.bin").write_bytes(bail.recu)
                        Path("surface.bin").write_bytes(lease)
                        Path("diagnostic.efi").write_bytes(pe)
                        offset_entry = (
                            meta["entree"] - meta["texte"][0] + meta["texte"][1]
                        )
                        info.update(
                            prets=ledger.loans,
                            emission_ns=grant_ns,
                            empreinte_contrat=empreinte(canon(contract)),
                            avant_start=dict(
                                position=trace_marks(), sections=sections, base=base
                            ),
                        )
                        tentry = {}

                        def entree(mon, dbg):
                            raw_entry = live(
                                hw, state(mon)["unites"][unit], old, ledger
                            )
                            save_tables("entree.bin", raw_entry)
                            rsp = struct.unpack_from(
                                "<17Q", bytes.fromhex(dbg.command("g"))
                            )[7]
                            exiger(
                                0 < rsp < reserve and raw_entry == prepare,
                                "Pile ou traduction à l'entrée",
                            )
                            info["pile_entree"] = rsp
                            exiger(
                                all(
                                    empreinte(ram(mon, base + r, s)) == h
                                    for r, s, h in sections.values()
                                ),
                                "Sections différentes à l'entrée",
                            )
                            exiger(
                                ram(mon, w[12] + size + 16384 - 344, 344)
                                == lease + bail.recu,
                                "Reçus en RAM",
                            )
                            projeter(mon, surface, "avant.ppm")
                            tentry["front"] = ram(mon, surface[0], surface[1])
                            tentry["stats"] = count(mon)
                            tentry["t"] = time.monotonic_ns()

                        if case in ("signature", "contradiction", "perime"):
                            front = ram(q, surface[0], surface[1])
                            hw.ecrire(box + 16, 5)
                            q.call("cont")
                            limit = time.monotonic() + 10
                            while hw.lire(box + 16) != 6:
                                exiger(
                                    time.monotonic() < limit,
                                    "Contrat altéré non refusé",
                                )
                                time.sleep(0.01)
                            q.call("stop")
                            exiger(
                                hw.lire(box + 29 * 8) == 0
                                and hw.lire(box + 27 * 8) == 2
                                and ram(q, surface[0], surface[1]) == front
                                and live(hw, state(q)["unites"][unit], old, ledger)
                                == prepare,
                                "Arrêt avant StartImage",
                            )
                            save(
                                "bilan.json",
                                dict(info, decision="refus-" + case, apres=count(q)),
                            )
                            return
                        hw.ecrire(box + 16, 5)
                        observer(
                            q,
                            j,
                            cal,
                            diagnostic=dict(
                                meta,
                                fichier=Path("diagnostic.efi"),
                                offset_entree=offset_entry,
                            ),
                            verifier_entree=entree,
                            evenements=TRACE
                            + (
                                "apm_io_write",
                                "pci_nvme_admin_cmd",
                                "pci_nvme_io_cmd",
                                "pci_nvme_mmio_doorbell_sq",
                                "pci_nvme_mmio_doorbell_cq",
                            ),
                        )
                        elapsed = time.monotonic_ns() - tentry["t"]
                        verdict = recv(q, base, w[25])
                        projeter(q, surface, "apres.ppm")
                        back = ram(q, surface[0], surface[1])
                        front = tentry["front"]
                        stride = surface[4] * 4
                        exiger(
                            hw.lire(box + 29 * 8) == 1
                            and verdict["epoque"] == epoch
                            and front != back
                            and all(
                                front[y * stride + 544 * 4 : (y + 1) * stride]
                                == back[y * stride + 544 * 4 : (y + 1) * stride]
                                for y in range(120)
                            )
                            and front[120 * stride :] == back[120 * stride :],
                            "Rectangle graphique",
                        )
                        fin = count(q)
                        interval = Path("intervalle.log").read_bytes()
                        exiger(
                            fin == tentry["stats"]
                            and fin["interne"] == counters["interne"]
                            and Path("interne.raw").read_bytes() == canary,
                            "E/S pendant le diagnostic",
                        )
                        exiger(
                            not re.search(
                                rb"(?m)^(?:usb_msd_cmd_submit|vtd_dmar_fault|vtd_reg_write|pci_nvme_|apm_io_write|virtio_notify)",
                                interval,
                            ),
                            "Commande inattendue",
                        )
                        exiger(
                            verdict["statut"] == 40
                            and verdict["confiance"] == w[23]
                            and live(hw, state(q)["unites"][unit], old, ledger)
                            == prepare,
                            "Verdict ou traduction finale",
                        )
                        final_tables = live(hw, state(q)["unites"][unit], old, ledger)
                        save_tables("apres.bin", final_tables)
                        cpu = Path("cpu.log").read_text().splitlines()
                        exiger(
                            cpu
                            == ["initialise"]
                            + [
                                f"marqueur cpu=0 cellule={c} valeur={v}"
                                for c, v in ((27, 1), (27, 2), (29, 1), (27, 3))
                            ],
                            "Écritures CPU non attribuées dans les tables ou marqueurs absents",
                        )
                        exiger(
                            not re.search(
                                rb"(?m)^vtd_reg_write ",
                                Path("trace.log").read_bytes()[info["debut_vcpu"] :],
                            ),
                            "Écriture de configuration VT-d du vCPU",
                        )
                        exiger(
                            empreinte(media.read_bytes()) == info["image"],
                            "Support modifié",
                        )
                        save(
                            "bilan.json",
                            dict(
                                info,
                                decision="refus-affiche",
                                verdict=verdict,
                                apres=fin,
                                duree_efi_ns=elapsed,
                                epoque_recu=bail.epoque.hex(),
                                source=nodes,
                                reads_before=tentry["stats"],
                                stockage=empreinte(canary),
                            ),
                        )
            finally:
                vm.terminate()
                vm.wait(timeout=15)
                if Path("bilan.json").is_file():
                    bilan = json.loads(Path("bilan.json").read_bytes())
                    bilan["image_apres"] = support_intact(media, bilan["image"])
                    exiger(
                        Path("interne.raw").read_bytes() == canary,
                        "Disque synthétique modifié",
                    )
                    bilan["stockage_apres"] = empreinte(canary)
                    save("bilan.json", bilan)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for n in ("sortie", "build5j", "firmware", "cle"):
        p.add_argument(n, type=Path)
    p.add_argument("--plugin", type=Path, required=True)
    p.add_argument("--mode", action="append", choices=["actif", "inactif"])
    p.add_argument(
        "--cas",
        action="append",
        choices=[
            "nominal",
            "vol",
            "invalide",
            "clone",
            "reconnexion",
            "signature",
            "contradiction",
            "perime",
        ],
    )
    a = p.parse_args()
    out = a.sortie.resolve()
    out.mkdir(mode=0o700)
    j = a.build5j.resolve()
    fw = k.exact(
        a.firmware.resolve(),
        FIRMWARE,
    )
    exe = k.exact(
        j / "modele5j/build/qemu-system-x86_64",
        "085ed32ca433172258c779258f435cb44388e5e6f64d9d8fd816c1fd4f27791e",
    )
    key = cle_autorisee(a.cle)
    media = out / "usb.img"
    creer(media, 68 * MIO, chargeur())
    creer(out / "clone.img", 68 * MIO, chargeur())
    with k.qualification(NOVA.parent.parent, {}):
        for mode in a.mode or ("actif", "inactif"):
            for case in a.cas or (
                "nominal",
                "vol",
                "invalide",
                "clone",
                "reconnexion",
                "signature",
                "contradiction",
                "perime",
            ):
                run(
                    out / (mode + "-" + case),
                    exe,
                    fw,
                    j,
                    media,
                    key,
                    mode,
                    case,
                    a.plugin.resolve(),
                )


if __name__ == "__main__":
    main()
