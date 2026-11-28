"""Exclusion du RID xHCI au reset, puis énumération USB ordinaire d’OVMF. Uniquement Q35."""

import argparse
from hashlib import sha256
import gzip
import json
from pathlib import Path
import re
import socket
import secrets
import subprocess
import sys
import time

from observation import NOVA, state, devices, require
from image5o import lecteur, creer, chargeur, PREUVES, MIO
from demarrage import FIRMWARE, directory, public_member, STORES, count

sys.path.insert(0, str(NOVA / "boot/diagnostic-5m"))
from banc5m import k


def run(out, exe, fw, image, mode):
    out.mkdir()
    epoch = secrets.token_hex(16)
    with directory(out), lecteur(image) as media:
        store = (
            gzip.decompress((PREUVES / "magasin-ovmf-test.fd.gz").read_bytes())
            if mode == "actif"
            else public_member("inactif.fd", STORES["inactif"])
        )
        Path("variables.fd").write_bytes(store)
        canary = bytes(range(256)) * 16384
        Path("interne.raw").write_bytes(canary)
        # La matrice fermée de bus et de tables est celle du témoin 5K.
        inherited = k.arguments_q35(
            exe, fw, Path("inutilise"), Path("variables.fd").resolve()
        )
        args = []
        for i in range(0, len(inherited)):
            if i and inherited[i - 1] in ("-fw_cfg", "-d"):
                continue
            if inherited[i] in ("-fw_cfg", "-d", "fw_cfg_io.dma_enabled=off"):
                continue
            if (
                inherited[i] == "-global"
                and inherited[i + 1] == "fw_cfg_io.dma_enabled=off"
            ):
                continue
            args.append(inherited[i])
        args += [
            "-add-fd",
            f"fd={media.fileno()},set=1,opaque=usb5n",
            "-drive",
            "if=none,id=usb5n,readonly=on,format=raw,file.driver=file,file.filename=/dev/fdset/1,file.locking=off",
            "-device",
            "qemu-xhci,id=xhci",
            "-device",
            "usb-storage,id=cle,bus=xhci.0,drive=usb5n,bootindex=1",
            "-boot",
            "strict=on",
            "-serial",
            "file:firmware.log",
            "-D",
            "transactions.log",
            "-trace",
            "usb_xhci_oper_write",
            "-trace",
            "usb_msd_cmd_submit",
            "-trace",
            "pci_nvme_admin_cmd",
        ]
        with (
            Path("qemu.log").open("xb") as err,
            subprocess.Popen(
                args,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=err,
                pass_fds=[media.fileno()],
                bufsize=0,
            ) as vm,
        ):
            try:
                q = k.Qmp(vm)
                devs = list(devices(q.call("query-pci")))
                require(
                    all(d["bus"] == 0 for d in devs), "Pont hors du modèle d'exclusion"
                )
                ids = {
                    d["qdev_id"]: 8 * d["slot"] + d["function"]
                    for d in devs
                    if d["qdev_id"]
                }
                rid = ids["xhci"]
                (unit,) = state(q)["unites"]
                (dimm,) = q.call("query-memory-devices")
                require(
                    dimm["data"]["id"] == "labo" and dimm["data"]["addr"] >= 1 << 32,
                    "Réserve inattendue",
                )
                with socket.socket(socket.AF_UNIX) as sock:
                    sock.connect("qt")
                    with sock.makefile("rwb", buffering=0) as io:
                        k.couverture(
                            k.dmar(io),
                            int(unit, 16),
                            {8 * d["slot"] + d["function"] for d in devs},
                        )
                        hw = k.Materiel(q, io, int(unit, 16), dimm["data"]["addr"], ids)
                        t = time.monotonic_ns()
                        mapping = hw.installer()
                        before = b"".join(hw.tables().values())
                        setup = time.monotonic_ns() - t
                        require(
                            rid not in mapping and ids["stock"] in mapping,
                            "Portée des domaines",
                        )
                        regs = state(q)["unites"][unit]
                        require(regs["gsts"] & 0x80000000, "TES absent")
                        stats = count(q)
                        q.call("cont")
                        deadline = time.monotonic() + 25
                        filter_fault = (
                            rb"(?m)^vtd_dmar_fault sid "
                            + hex(rid).encode()
                            + rb" fault \d+ addr 0x[0-9a-f]+ write [01]$"
                        )
                        while not re.search(
                            filter_fault, Path("transactions.log").read_bytes()
                        ):
                            require(
                                time.monotonic() < deadline,
                                "Faute du RID xHCI non observée",
                            )
                            time.sleep(0.02)
                        q.call("stop")
                        after = b"".join(hw.tables().values())
                        end = state(q)["unites"][unit]
                        io_end = count(q)
                        require(
                            before == after
                            and end["rtaddr"] == regs["rtaddr"]
                            and end["gsts"] & 0x80000000,
                            "Protection du domaine modifiée",
                        )
                        require(
                            io_end == stats
                            and Path("interne.raw").read_bytes() == canary,
                            "E/S interne ou lecture de la clé malgré exclusion",
                        )
                        raw = Path("transactions.log").read_bytes()
                        require(
                            b"usb_msd_cmd_submit " not in raw,
                            "Commande USB malgré exclusion",
                        )
                        Path("tables.bin").write_bytes(before)
                        Path("bilan.json").write_text(
                            json.dumps(
                                dict(
                                    mode=mode,
                                    epoque=epoch,
                                    rids=ids,
                                    unite=regs,
                                    unite_fin=end,
                                    domaines=mapping,
                                    octets_tables=len(before),
                                    installation_ns=setup,
                                    tables_sha256=sha256(before).hexdigest(),
                                    avant=stats,
                                    apres=io_end,
                                    fautes_xhci=[
                                        s.decode()
                                        for s in re.findall(filter_fault, raw)
                                    ],
                                    stockage_sha256=sha256(canary).hexdigest(),
                                    decision="aucun transfert",
                                    limite="fw_cfg et autorité du banc hors du domaine PCI xHCI",
                                ),
                                indent=2,
                            )
                            + "\n"
                        )
            finally:
                vm.terminate()
                vm.wait(timeout=10)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for n in ("sortie", "build5j", "firmware"):
        p.add_argument(n, type=Path)
    a = p.parse_args()
    out = a.sortie.resolve()
    out.mkdir(mode=0o700)
    exe = k.exact(
        a.build5j.resolve() / "modele5j/build/qemu-system-x86_64",
        "085ed32ca433172258c779258f435cb44388e5e6f64d9d8fd816c1fd4f27791e",
    )
    fw = k.exact(a.firmware.resolve(), FIRMWARE)
    image = out / "usb.img"
    creer(image, 68 * MIO, chargeur())
    with k.qualification(NOVA.parent.parent, {}):
        for mode in ("actif", "inactif"):
            run(out / mode, exe, fw, image, mode)


if __name__ == "__main__":
    main()
