"""Q35 USB 5P. Les disques sont des fichiers privés, les sources USB ouvertes en lecture seule."""

import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time
from contextlib import ExitStack
from concurrent.futures import ThreadPoolExecutor

from origine import provenance
from temoin_usb import partition, chemin_usb, tampons, garde, lecture_hotplug
from observation import NOVA, activite, instantane
from image5o import PREUVES, chargeur, lecteur, verifier, MIO, creer
from demarrage import (
    Debug,
    Qmp,
    count,
    ram,
    record,
    require,
    STORES,
    FIRMWARE,
    public_member,
    directory,
)
from image import pe_info, sections_chargees

sys.path.insert(0, str(NOVA / "tests/ethernet_v1"))
from port_qemu import qualification

EV = (
    "usb_xhci_slot_address",
    "usb_xhci_ep_enable",
    "usb_xhci_fetch_trb",
    "usb_xhci_queue_event",
    "usb_xhci_xfer_start",
    "usb_xhci_xfer_async",
    "usb_xhci_xfer_success",
    "usb_xhci_xfer_error",
    "usb_msd_cmd_submit",
    "usb_msd_cmd_complete",
    "usb_xhci_oper_write",
    "usb_xhci_runtime_write",
    "ide_bus_exec_cmd",
    "ahci_cmd_done",
    "vtd_reg_write",
    "vtd_dmar_fault",
)
PROFILS = ("direct", "hub", "bochs", "double", "reconnexion", "en-vol")


def h(b):
    return hashlib.sha256(b).hexdigest()


def save(path, data):
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def ecran(q, nom):
    q.call("screendump", filename=nom)
    data = Path(nom).read_bytes()
    magic, shape, maximum, rgb = data.split(b"\n", 3)
    w, height = map(int, shape.split())
    require(
        magic == b"P6" and maximum == b"255" and len(rgb) == w * height * 3,
        "Géométrie de la console",
    )
    return [w, height, h(data)]


def reprise_en_vol(dbg, q, trace, epoch):
    # GDB observe le premier RIP. Le moniteur ne suspend pas la VM pendant la requête USB.
    with ThreadPoolExecutor(max_workers=1) as thread:
        running = thread.submit(dbg.resume)
        limit = time.monotonic() + 35
        while not running.done() and time.monotonic() < limit:
            raw = trace.read_bytes()
            if raw.endswith(b"\n") and raw.rsplit(b"\n", 2)[-2].startswith(
                b"usb_xhci_xfer_async "
            ):
                state = activite(raw.decode().splitlines())
                if (
                    any(e > 1 and pending for _, e, pending in state["vols"])
                    and not state["anomalies"]
                ):
                    status = q.call("query-status")["status"]
                    if status == "running" and trace.read_bytes() == raw:
                        witness = dict(
                            position=len(raw),
                            activite=state,
                            decision="quarantaine",
                            epoque=epoch,
                            observation="VM en cours",
                            etat_qmp=status,
                        )
                        running.result(timeout=35)
                        return witness
            time.sleep(0.003)
        running.result(timeout=35)
        raise ValueError("Requête USB en vol non observée pendant la marche")


def mesurer(folder, media, clone, fw, mode, profil, cal=None):
    folder.mkdir(mode=0o700)
    pe = chargeur()
    meta = pe_info(pe)
    with directory(folder), ExitStack() as stack:
        epoch = secrets.token_hex(16)
        store = (
            gzip.decompress((PREUVES / "magasin-ovmf-test.fd.gz").read_bytes())
            if mode == "actif"
            else public_member("inactif.fd", STORES["inactif"])
        )
        require(
            h(store)
            == (
                "bf41a49e08223f8e0a0192371d8fb79602faf56e4c0a58a5ad79624a54f2ed8f"
                if mode == "actif"
                else STORES["inactif"]
            ),
            "Magasin non qualifié",
        )
        Path("variables.fd").write_bytes(store)
        canary = bytes(range(256)) * 16384
        Path("interne.raw").write_bytes(canary)
        args = [
            "qemu-system-x86_64",
            "-machine",
            "q35,smm=on",
            "-accel",
            "tcg,thread=single",
            "-cpu",
            "max",
            "-m",
            "128M",
            "-smp",
            "1",
            "-nodefaults",
            "-display",
            "none",
            "-serial",
            "file:firmware.log",
            "-monitor",
            "none",
            "-qmp",
            "stdio",
            "-gdb",
            "unix:observateur.sock,server=on,wait=off",
            "-S",
            "-no-reboot",
            "-no-shutdown",
            "-boot",
            "strict=on",
            "-nic",
            "none",
            "-drive",
            f"if=pflash,unit=0,readonly=on,format=raw,file={fw}",
            "-drive",
            "if=pflash,unit=1,snapshot=on,format=raw,file=variables.fd",
            "-global",
            "driver=cfi.pflash01,property=secure,value=on",
            "-device",
            "intel-iommu",
            "-drive",
            "if=none,id=interne,format=raw,file=interne.raw",
            "-device",
            "ide-hd,id=sata,drive=interne,bus=ide.0",
            "-D",
            "transactions.log",
        ]
        hub = profil == "hub"
        vid = (
            "bochs-display,id=video,xres=1024,yres=768"
            if profil == "bochs"
            else "VGA,id=video,romfile=,xres=1280,yres=720"
        )
        args += [
            "-device",
            vid,
            "-device",
            ("nec-usb-xhci" if hub else "qemu-xhci") + ",id=usbA",
        ]
        if hub:
            args += ["-device", "usb-hub,id=hub,bus=usbA.0,port=1"]
        if profil == "double":
            args += ["-device", "nec-usb-xhci,id=usbB"]
        handles = []
        for i, p in enumerate(
            (media, clone) if profil in ("double", "reconnexion") else (media,), 1
        ):
            f = stack.enter_context(lecteur(p))
            handles.append(f.fileno())
            name = "usb5n" if i == 1 else "clone5n"
            drive = f"if=none,id={name},format=raw,readonly=on,file.driver=file,file.filename=/dev/fdset/{i},file.locking=off"
            if profil == "en-vol":
                drive += ",throttling.bps-read=131072"
            args += ["-add-fd", f"fd={f.fileno()},set={i},opaque={name}"]
            if profil == "reconnexion":
                # La durée de vie du nœud bloc dépasse celle du périphérique débranché.
                args += [
                    "-blockdev",
                    f"driver=file,node-name=fichier{i},filename=/dev/fdset/{i},locking=off,read-only=on",
                    "-blockdev",
                    f"driver=raw,node-name={name},file=fichier{i},read-only=on",
                ]
            else:
                args += ["-drive", drive]
            if i == 1 or profil == "double":
                bus = "usbA" if i == 1 else "usbB"
                args += [
                    "-device",
                    f"usb-storage,id=cle{i},bus={bus}.0,port="
                    + ("1.2" if hub else "1")
                    + f",drive={name}"
                    + (",bootindex=1" if i == 1 else ""),
                ]
        for event in (
            EV
            + ("blk_co_preadv", "blk_co_pwritev", "blk_root_attach", "blk_root_detach")
            if profil == "reconnexion"
            else EV
        ):
            args += ["-trace", event]
        trace = Path("transactions.log")

        def lines():
            return trace.read_text(errors="strict").splitlines()

        with (
            Path("qemu.log").open("xb") as log,
            subprocess.Popen(
                args,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=log,
                pass_fds=handles,
                bufsize=0,
            ) as vm,
        ):
            dbg = None
            try:
                q = Qmp(vm)

                def compteurs():
                    stats = count(q)
                    if profil == "reconnexion":
                        for row in q.call("query-blockstats"):
                            name = row.get("node-name")
                            if name in ("usb5n", "clone5n"):
                                stats[name] = {
                                    k: row["stats"][k] for k in stats["interne"]
                                }
                    return stats

                reset = compteurs()
                if profil == "hub":
                    # Point distinct de l’entrée du PE. L'énumération derrière le hub a son propre oracle.
                    q.call("cont")
                    limit = time.monotonic() + 30

                    def enfant_visible():
                        brut = trace.read_bytes()
                        address = re.search(
                            rb"usb_xhci_slot_address slotid (\d+), port 1\.2\n", brut
                        )
                        return (
                            address
                            and b"usb_xhci_ep_enable slotid "
                            + address[1]
                            + b", epid 4\n"
                            in brut
                        )

                    while not enfant_visible():
                        require(time.monotonic() < limit, "Endpoints du hub absents")
                        time.sleep(0.04)
                    time.sleep(0.45)
                    q.call("stop")
                    inv = instantane(q)
                    (ctrl,) = inv["controleurs"].values()
                    (hubslot,) = [s for s in ctrl["slots"] if s["route"] == 0]
                    (child,) = [s for s in ctrl["slots"] if s["route"] == 2]
                    require(
                        child["port_racine"] == hubslot["port_racine"]
                        and {e["type"] for e in child["endpoints"]} >= {2, 4, 6},
                        "Route hub incomplète",
                    )
                    acts = activite(lines())
                    require(
                        [child["slot"], "1.2"] in acts["adresses"]
                        and acts["luns"] == [0],
                        "Slot ou LUN du support",
                    )
                    stats = compteurs()
                    require(
                        Path("interne.raw").read_bytes() == canary,
                        "Contenu SATA altéré",
                    )
                    entrees = [x for x in lines() if x.startswith("ide_bus_exec_cmd ")]
                    # Le test du hub consigne le contre-exemple avant toute autorisation de transfert.
                    echec_io = stats["interne"] != reset["interne"] or bool(entrees)
                    save(
                        Path("bilan.json"),
                        dict(
                            profil=profil,
                            mode=mode,
                            epoque=epoch,
                            magasin=h(store),
                            inventaire=inv,
                            activite=acts,
                            compteurs=stats,
                            reset=reset,
                            observation="énumération avant attribution du premier EFI",
                            restitution=False,
                            decision="refus",
                            echec_io=echec_io,
                            commandes_sata=len(entrees),
                            stockage_sha256=h(canary),
                        ),
                    )
                    return
                if cal is None:
                    q.call("cont")
                    limit = time.monotonic() + 60
                    while b"BdsDxe: starting" not in Path("firmware.log").read_bytes():
                        require(
                            time.monotonic() < limit, "Démarrage de calibration absent"
                        )
                        time.sleep(0.05)
                    time.sleep(0.4)
                    q.call("stop")
                    data = ram(q, 0, 128 << 20)
                    record(data, 0)
                    rva, off, _ = meta["texte"]
                    bases = [
                        m.start() - rva
                        for m in re.finditer(re.escape(pe[off : off + 64]), data)
                        if m.start() % 4096 == 0
                        and data[m.start() - rva : m.start() - rva + 2] == b"MZ"
                    ]
                    require(len(bases) == 1, "Image de calibration ambiguë")
                    return bases[0]
                entry, halt = cal + meta["entree"], cal + meta["hlt"]
                dbg = Debug()
                dbg.watch(entry)
                dbg.watch(halt)
                pending = None
                if profil == "en-vol":
                    pending = reprise_en_vol(dbg, q, trace, epoch)
                else:
                    dbg.resume()
                require(dbg.pc() == entry, "Premier RIP")
                dbg.remove(entry)
                start = time.monotonic_ns()
                before = compteurs()
                cut = trace.stat().st_size
                inv = instantane(q)
                screen = ecran(q, "avant.ppm")
                links = q.call("query-block")
                fdsets = q.call("query-fdsets")
                supports = {}
                for r in links:
                    inserted = r.get("inserted", {})
                    for n in (r.get("device"), inserted.get("node-name")):
                        if n in ("usb5n", "clone5n") and inserted.get("ro"):
                            supports[n] = inserted["file"]
                require(
                    supports.get("usb5n") == "/dev/fdset/1"
                    and any(
                        x["fdset-id"] == 1 and x["fds"][0]["opaque"] == "usb5n"
                        for x in fdsets
                    ),
                    "Origine fichier",
                )
                sections = {}
                for name, (rva, size, digest) in sections_chargees(pe, cal).items():
                    observed = h(ram(q, cal + rva, size))
                    require(observed == digest, "PE chargé différent")
                    sections[name] = [rva, size, observed]
                bds = [
                    x.split("BdsDxe: ", 1)[1]
                    for x in Path("firmware.log")
                    .read_text(errors="replace")
                    .splitlines()
                    if "BdsDxe: starting" in x
                ]
                require(len(bds) == 1 and "/USB(" in bds[0], "Origine du démarrage")
                loaded = provenance(q, dbg, inv["ram"], cal, meta["taille"])
                prefix = trace.read_bytes()[:cut]
                with lecteur(media) as source:
                    gpt = partition(source)
                bus = q.call(
                    "qom-get", path="/machine/peripheral/cle1", property="parent_bus"
                )
                identite = chemin_usb(inv, loaded, bds[0], gpt, bus)
                if pending:
                    require(
                        activite(prefix[: pending["position"]].decode().splitlines())
                        == pending["activite"],
                        "Coupe en vol différente",
                    )
                buffers = tampons(prefix, inv["ram"], cal, meta["taille"])
                change = None
                reference = before
                if profil == "reconnexion":
                    # Hot-unplug USB sur le modèle arrêté. Le clone réemploie le même port.
                    q.call("device_del", id="cle1")
                    start_add = trace.stat().st_size
                    q.call(
                        "device_add",
                        driver="usb-storage",
                        id="remplacement",
                        bus="usbA.0",
                        port="1",
                        drive="clone5n",
                    )
                    end_add = trace.stat().st_size
                    require(
                        dbg.pc() == entry,
                        "vCPU hors du premier RIP pendant le remplacement",
                    )
                    change = dict(
                        debut_ajout=start_add,
                        fin_ajout=end_add,
                        apres=q.call(
                            "human-monitor-command", **{"command-line": "info usb"}
                        )
                        .strip()
                        .splitlines(),
                        decision="origine-perdue",
                        generation_avant=1,
                        generation_apres=2,
                    )
                    reference = compteurs()
                    require(
                        before["usb5n"]["rd_bytes"] > 0
                        and set(reference) == {"clone5n", "interne"}
                        and not any(reference["clone5n"].values()),
                        "Compteurs des deux incarnations",
                    )
                    change["compteurs_nouvelle_incarnation"] = reference
                debug_t = time.monotonic_ns()
                dbg.resume()
                require(dbg.pc() == halt, "Arrêt terminal")
                duree = time.monotonic_ns() - debug_t
                end = trace.stat().st_size
                after = compteurs()
                verdict = record(ram(q, cal, meta["taille"]), cal)
                screen_after = ecran(q, "apres.ppm")
                require(
                    screen_after == screen and reference == after,
                    "Écran ou E/S changés",
                )
                raw = trace.read_bytes()
                interval = raw[cut:end]
                require(
                    not any(
                        x in interval
                        for x in (
                            b"ide_bus_exec_cmd",
                            b"ahci_cmd_done",
                            b"vtd_reg_write",
                            b"usb_msd_cmd_submit",
                            b"blk_co_pwritev",
                        )
                    ),
                    "Commande pendant le diagnostic",
                )
                if change:
                    change["lecture_modele_octets"] = lecture_hotplug(
                        raw, cut, end, start_add, end_add
                    )
                require(
                    Path("interne.raw").read_bytes() == canary
                    and reset["interne"] == before["interne"]
                    and not re.search(
                        rb"(?m)^(?:ide_bus_exec_cmd|ahci_cmd_done) ", raw[:end]
                    ),
                    "Témoin SATA sollicité ou altéré",
                )
                require(
                    verdict["statut"] == 16 and verdict["confiance"] == 2,
                    "Arrêt conservateur USB",
                )
                require(
                    activite(prefix.decode().splitlines())["luns"] == [0],
                    "LUN inattendu",
                )
                obs = dict(
                    epoque=epoch,
                    profil=profil,
                    mode=mode,
                    pe=h(pe),
                    sections_lues=sections,
                    magasin=h(store),
                    origine=bds[0],
                    loaded_image=loaded,
                    inventaire=inv,
                    liaison_qom=bus,
                    gpt=gpt,
                    temoin=garde(epoch, identite, supports, change, pending),
                    tampons=buffers,
                    base=cal,
                    taille_image=meta["taille"],
                    sources=supports,
                    avant=before,
                    apres=after,
                    reset=reset,
                    rip=entry,
                    hlt=halt,
                    ecran=screen,
                    ecran_apres=screen_after,
                    verdict=verdict,
                    vol=pending,
                    changement=change,
                    trace_entree=cut,
                    trace_fin=end,
                    activite=activite(raw[:cut].decode().splitlines()),
                    intervalle_ns=duree,
                    observation_ns=time.monotonic_ns() - start,
                    stockage_sha256=h(canary),
                    vm="Q35 TCG",
                    restitution=False,
                )
                save(Path("bilan.json"), obs)
                return obs
            finally:
                vm.terminate()
                vm.wait(timeout=15)
                if dbg:
                    dbg.sock.close()


def preparer(out):
    out.mkdir(mode=0o700)
    original = gzip.decompress((PREUVES / "novaos-x86_64-68mio.img.gz").read_bytes())
    require(
        h(original)
        == "a4dd3d96739b0821f955a4e4e14524516b100e557d333739a6cecda23d31d7cb",
        "Identité image",
    )
    for name in ("media.img", "clone.img"):
        p = out / name
        creer(p, 68 * MIO, chargeur())
        require(h(p.read_bytes()) == h(original), "Image générique non reproduite")
        with lecteur(p) as f:
            verifier(f, 68 * MIO, chargeur())
    return h(original)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie", type=Path)
    p.add_argument("firmware", type=Path)
    p.add_argument("--profil", action="append", choices=PROFILS)
    p.add_argument("--mode", action="append", choices=("actif", "inactif"))
    a = p.parse_args()
    out = a.sortie.resolve()
    fw = a.firmware.resolve()
    require(h(fw.read_bytes()) == FIRMWARE, "Firmware inattendu")
    attendu = preparer(out)
    rapport = {}
    with qualification(NOVA.parent.parent, rapport):
        for profil in a.profil or PROFILS:
            for mode in a.mode or ("actif", "inactif"):
                args = (out / "media.img", out / "clone.img", fw, mode, profil)
                if profil == "hub":
                    mesurer(out / f"{profil}-{mode}", *args)
                else:
                    cal = mesurer(out / f"cal-{profil}-{mode}", *args)
                    mesurer(out / f"{profil}-{mode}", *args, cal)
                digests = {
                    n: h((out / n).read_bytes()) for n in ("media.img", "clone.img")
                }
                require(set(digests.values()) == {attendu}, "Média ou journal modifié")
                save(
                    out / f"{profil}-{mode}" / "integrite.json",
                    dict(avant=attendu, apres=digests),
                )


if __name__ == "__main__":
    main()
