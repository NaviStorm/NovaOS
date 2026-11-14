"""Démarrer les sondes EL0 et IPC sur un disque GPT jetable de l'application UTM."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess

from es_groupees_utm import convertir, image, ouvrir_moniteur
from nsh_utm import demarrer, enregistrer, nommer


def run(template, elf, generator, out, scenario):
    out.mkdir()
    volume = out / "volume.raw"
    subprocess.run([generator, "--image", volume, "4096", "utm"], check=True)
    disk = out / "systeme.raw"
    image(disk, volume, elf, 4096, scenario=scenario)
    bundle = out / "sonde.utm"
    data = bundle / "Data"
    data.mkdir(parents=True)
    cfg = plistlib.loads((template / "config.plist").read_bytes())
    assert [d["ImageName"] for d in cfg["Drive"]] == ["code.fd", "variables.fd", "cible.qcow2"]
    cfg["System"].update(CPUCount=4, ForceMulticore=True)
    assert "gic-version=3,its=off" in cfg["QEMU"]["MachinePropertyOverride"]
    args = cfg["QEMU"]["AdditionalArguments"]
    drive, = (i for i, val in enumerate(args) if val.startswith("virtio-blk-pci,"))
    args[drive] += ",logical_block_size=4096,physical_block_size=4096"
    for file in ("code.fd", "variables.fd"):
        shutil.copyfile(template / "Data" / file, data / file)
    convertir(disk, "raw", data / "cible.qcow2", "qcow2")
    (bundle / "config.plist").write_bytes(plistlib.dumps(cfg))
    bundle, ident = nommer(bundle, scenario)
    cfg = plistlib.loads((bundle / "config.plist").read_bytes())
    cfg["Serial"].append(dict(Mode="Ptty", Target="Monitor"))
    (bundle / "config.plist").write_bytes(plistlib.dumps(cfg))
    serial = out / "sonde.serial"
    with enregistrer(bundle, ident) as machine, demarrer(machine, serial) as c, ExitStack() as stack:
        monitor = ouvrir_moniteur(machine, out, stack)
        os.write(monitor.fd, b"info cpus\r")
        response = monitor.attendre(b"thread_id=", 15) + monitor.attendre(b"(qemu) ", 15)
        rows = [(int(cpu), int(tid)) for cpu, tid in re.findall(rb"CPU #(\d+): thread_id=(\d+)", response)]
        assert sorted(cpu for cpu, _ in rows) == list(range(4)), response
        marker = {"smp-test": b"rendezvous=0xf actifs=4 OK", "smp-fp": b"cycle=1 clients=8 migrations=8 OK",
                  "smp-arret": b"ack=0xf absents=0x0 muets=0x0"}[scenario]
        c.attendre(marker, 90)
    with elf.open("rb") as f:
        sha = hashlib.file_digest(f, "sha256").hexdigest()
    (out / "resultat.json").write_text(json.dumps(dict(elf=sha, scenario=scenario,
                       vcpu=rows, threads_moteur=len({tid for _, tid in rows}),
                       modele=cfg["System"], marque=marker.decode()), indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for field in ("template", "elf", "generator", "out"):
        parser.add_argument(field, type=Path)
    parser.add_argument("scenario", choices=("smp-test", "smp-fp", "smp-arret"))
    args = parser.parse_args()
    run(args.template.resolve(), args.elf.resolve(), args.generator.resolve(), args.out.resolve(), args.scenario)
