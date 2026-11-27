"""Photographier le témoin historique « domaines fermés » du Q35, VM arrêtée."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

from domaines import legacy
from sonde import cpu_flatview, hmp, state

NOVA = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(NOVA / "boot/diagnostic-5e"), str(NOVA / "tests/ethernet_v1")]
from banc import Qmp, digest
from port_qemu import qualification


def exact(path, want):
    if not path.is_file() or digest(path) != want:
        raise ValueError("Entrée de laboratoire différente " + path.name)
    return path


def capture(q, folder, allowed):
    folder.mkdir()
    # Contrat du témoin arrêté. Exiger en plus un FlatView CPU qui désigne de la RAM.
    tree = hmp(q, "info mtree -f")
    (folder / "flatview.txt").write_text(tree)
    cpu = cpu_flatview(tree)
    lines = re.findall(
        r"(?m)^\s*([0-9a-f]+)-([0-9a-f]+) \(prio -?\d+, ram\): pc.ram(?: |$)", cpu
    )
    ranges = [(int(a, 16), int(b, 16)) for a, b in lines]
    pages = {}
    for addr in sorted(allowed):
        if not any(a <= addr and addr + 4095 <= b for a, b in ranges):
            raise ValueError("Cadre hors de la RAM CPU du modèle")
        path = folder / f"{addr:x}.bin"
        q.call("pmemsave", val=addr, size=4096, filename=str(path.resolve()))
        data = path.read_bytes()
        if len(data) != 4096:
            raise ValueError("Capture RAM incomplète")
        pages[addr] = data
    return pages


def save(path, obj):
    path.write_text(json.dumps(obj, indent=2, ensure_ascii=False) + "\n")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for k in ("archives", "sortie", "worktrees"):
        p.add_argument(k, type=Path)
    a = p.parse_args()
    base, out, trees = (p.resolve() for p in (a.archives, a.sortie, a.worktrees))
    fixture = base / "q35-duo-domaines-03"
    ref = fixture / "qemu"
    # Empreintes du profil publiées avant 5I. Ne jamais exécuter une liste d'arguments libre.
    old = json.loads(
        exact(
            ref / "campagne.json",
            "8d1fb7d5bb24a1457a899e9ded1a3e69219b87c30b8a0b6e4e30a344102a8200",
        ).read_text()
    )
    args = json.loads(
        exact(
            ref / "arguments.json",
            "ecbdebf288ed60f6e07cb0c611743a885965a8a5a9417a43c96bbc820e62b88f",
        ).read_text()
    )
    args[0] = str(
        exact(
            base / "modele-acpi-pio/modeles/qemu-system-x86_64",
            "f46d87c83a72ddea961b2b02a357c66b34c8b424f5613658262567e516f79f99",
        )
    )
    fw = exact(base / "q35-pio-firmware-02/OVMF_CODE.fd", old["firmware_sha256"])
    fat = exact(ref / "boot.fat", old["fat_sha256"])
    var = exact(fixture / "variables.fd", old["variables_sha256"])
    out.mkdir(parents=True, exist_ok=False)
    os.chdir(out)
    shutil.copyfile(var, "variables.fd")
    # Ce profil s'arrête avec BME=0. Le namespace témoin est un fichier neuf.
    with open("namespace.raw", "xb") as disk:
        disk.truncate(4 * 1024 * 1024)
    diskhash = digest(Path("namespace.raw"))
    for i, opt in enumerate(args[:-1]):
        v = args[i + 1]
        if opt == "-drive":
            src = (
                fw
                if "unit=0," in v
                else "variables.fd"
                if "unit=1," in v
                else "namespace.raw"
            )
            args[i + 1] = re.sub(r"file=[^,]+", lambda _: "file=" + str(src), v)
        elif opt == "-serial":
            args[i + 1] = "file:serie.log"
        elif opt == "-debugcon":
            args[i + 1] = "file:efi.log"
        elif opt == "-fw_cfg":
            args[i + 1] = "name=opt/novaos/boot.fat,file=" + str(fat)
    # Liste fermée, indépendante des pointeurs matériels. Géométrie du SEUL témoin
    # historique (40 cadres contigus du superviseur) ; aucune adresse dans l'EFI.
    root = old["duo"]["registres"]["32"]
    permitted = {root + 4096 * i for i in (*range(6), *range(14, 18))}
    data = {v & ~4095 for row in old["duo"]["feuilles"].values() for v in row.values()}
    result = {
        "modele": digest(Path(args[0])),
        "boot_fat": digest(fat),
        "magasin": digest(var),
        "firmware": digest(fw),
        "tables_autorisees": sorted(permitted),
        "donnees_autorisees": sorted(data),
        "garde": {},
    }
    with qualification(trees, result["garde"]):
        with (
            open("trace.log", "xb") as log,
            subprocess.Popen(
                args,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=log,
                bufsize=0,
            ) as vm,
        ):
            try:
                q = Qmp(vm)
                q.call("cont")
                stop = time.monotonic() + 25
                while time.monotonic() < stop:
                    if b"SMP arret motif=normal" in Path("serie.log").read_bytes():
                        break
                    time.sleep(0.04)
                else:
                    raise ValueError("Le témoin des domaines ne s'est pas arrêté")
                q.call("stop")
                if q.call("query-status")["running"]:
                    raise ValueError("VM non suspendue")
                for event in (
                    "memory_region_ops_write",
                    "memory_region_subpage_write",
                    "vtd_reg_write",
                ):
                    q.call("trace-event-set-state", name=event, enable=True)
                trace_begin = Path("trace.log").stat().st_size
                start = time.perf_counter_ns()
                before = state(q)
                first = capture(q, Path("avant"), permitted)
                stats = q.call("query-blockstats")
                units = before["unites"]
                if len(units) != 1:
                    raise ValueError("Nombre de remappeurs du témoin")
                domains = legacy(next(iter(units.values())), first, data)
                elapsed = time.perf_counter_ns() - start
                second = capture(q, Path("apres"), permitted)
                after = state(q)

                def io(rows):
                    return {
                        r["device"]: {
                            k: v
                            for k, v in r["stats"].items()
                            if k.endswith(("_operations", "_bytes"))
                        }
                        for r in rows
                        if r.get("device") == "stockage"
                    }

                io_a, io_b = io(stats), io(q.call("query-blockstats"))
                if (
                    before != after
                    or first != second
                    or not io_a
                    or io_a != io_b
                    or any(io_b["stockage"].values())
                    or trace_begin != Path("trace.log").stat().st_size
                ):
                    raise ValueError("Mutation pendant la photographie")
                if set(domains) != {48, 56}:
                    raise ValueError("Demandes inattendues")
                for sid, group in domains.items():
                    oracle = old["duo"]["feuilles"][str(sid)]
                    base_iova = {48: 0x02000000, 56: 0x10000000}[sid]
                    if (
                        group["did"] != {48: 1, 56: 3}[sid]
                        or {
                            str((m["iova"] - base_iova) // 4096): m["cadre"]
                            | m["droits"]
                            for m in group["mappings"]
                        }
                        != oracle
                    ):
                        raise ValueError("Écart avec l'oracle des feuilles")
                receipts = re.findall(
                    r"DUO WAIT sid=(\d+) did=(\d+) instance=(\d+) generation=(\d+) publication=true cookie=(\d+) iqt=(\d+)",
                    Path("serie.log").read_text(),
                )
                if [list(map(int, row)) for row in receipts] != [
                    [48, 1, 1, 1, 2, 96],
                    [56, 3, 2, 1, 3, 144],
                ]:
                    raise ValueError("Lien du titulaire absent")
                for sid in domains:
                    rid = f"0000:{sid >> 8:02x}:{sid >> 3 & 31:02x}.{sid & 7}"
                    if before["pci"][rid]["cmd"] & 4:
                        raise ValueError("BME témoin inattendu")
                result.update(
                    registres=before,
                    domaines=domains,
                    recus=receipts,
                    io=io_b,
                    ns_capture=elapsed,
                    trace_avant=trace_begin,
                    trace_apres=Path("trace.log").stat().st_size,
                    pages={
                        hex(k): hashlib.sha256(v).hexdigest() for k, v in first.items()
                    },
                )
            finally:
                vm.terminate()
                vm.wait(timeout=10)
        if digest(Path("namespace.raw")) != diskhash or digest(
            Path("variables.fd")
        ) != digest(var):
            raise ValueError("Support modifié")
        result["namespace"] = diskhash
        # Tester explicitement la limite du modèle sans exécuter de firmware.
        denial = subprocess.run(
            [
                "qemu-system-x86_64",
                "-machine",
                "q35",
                "-S",
                "-nodefaults",
                "-display",
                "none",
                "-device",
                "intel-iommu",
                "-device",
                "intel-iommu",
            ],
            capture_output=True,
            text=True,
            timeout=5,
        )
        if not denial.returncode or "multiple" not in denial.stderr.lower():
            raise ValueError("La limite Q35 multi-IOMMU doit être réexaminée")
        result["deux_modeles"] = denial.stderr.strip()
    save(Path("bilan.json"), result)


if __name__ == "__main__":
    main()
