"""Injecter les IRQ parasites et le délai réel sur une archive de laboratoire IRQ."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
from subprocess import DEVNULL, PIPE, Popen, run
import time

from bloc import qmp
from bloc_qemu import command


def preuve(archive, arch, secteur, sortie):
    sortie.mkdir(parents=True, exist_ok=False)
    nova = archive/"novaOS"
    env = {**os.environ, "NOVA_ARCH": arch}
    cible = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}[arch]
    noyau = nova/"target"/cible/"release/novaos"
    iso = sortie/"dma-qual.iso"
    run(["sh", nova/"scripts/iso.sh", "dma-qual", iso], env=env, check=True,
        stdout=DEVNULL, stderr=DEVNULL)
    image = sortie/"jetable.raw"
    motif = bytearray(1 << 20)
    motif[8192:12288] = bytes(0x71 + i % 83 for i in range(4096))
    image.write_bytes(motif)
    trace = sortie/"irq.serial"
    controls = ["-display", "none", "-monitor", "none", "-serial", f"file:{trace}", "-qmp", "stdio"]
    if arch == "aarch64":
        controls += ["-machine", "gic-version=3,its=off"]
    args = command(arch, secteur, image, readonly=True, debug="", controls=controls)
    with (sortie/"qemu.log").open("xb") as log, Popen(args, env={**env, "NOVA_ISO": str(iso)},
            stdin=PIPE, stdout=PIPE, stderr=log, bufsize=0) as vm:
        try:
            qmp(vm, "qmp_capabilities")
            def attendre(marque):
                limite = time.monotonic()+60
                while time.monotonic()<limite and vm.poll() is None:
                    flux = trace.read_text(errors="replace").rpartition("\n")[0] if trace.exists() else ""
                    if marque in flux:
                        return flux
                    if "Panique du noyau" in flux or "DMA bilan Err" in flux:
                        raise RuntimeError(flux[-1400:])
                    time.sleep(.005)
                raise RuntimeError(f"Marque absente {marque!r} dans {trace}")
            def controle(cmd):
                reponse = qmp(vm, "human-monitor-command", **{"command-line": f'qemu-io d "{cmd}"'})
                if reponse:
                    raise RuntimeError(f"{cmd} ({reponse})")
            if arch == "aarch64":
                for nombre in (1, 8):
                    attendre(f"IRQ-LAB armer nombre={nombre}")
                    controle("break read_aio arret")
                    attendre(f"IRQ-LAB SPI nombre={nombre}")
                    controle("wait_break arret")
                    if nombre == 8:
                        attendre("IRQ-LAB fin nombre=8 resultat=Err(Timeout)")
                    controle("resume arret")
            flux = attendre("DMA bilan Ok(())")
            assert "IRQ-LAB nouveau proprietaire lecture exacte et retrait" in flux
            profils = []
            for cas, resultat, ticks, hz, compte in re.findall(
                    r"IRQ-LAB (fin nombre=[18]|MSI-X) resultat=(\S+) ticks=(\d+) hz=(\d+) mesures=(\[[\d, ]+\])", flux):
                profils.append(dict(cas=cas, resultat=resultat, ticks=int(ticks), hz=int(hz),
                                    ms=round(1000*int(ticks)/int(hz),3), compteurs=json.loads(compte)))
            assert len(profils)==(2 if arch=="aarch64" else 1)
        finally:
            if vm.poll() is None:
                vm.terminate()
            vm.wait(timeout=10)
    assert image.read_bytes() == motif
    sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    resultat = dict(architecture=arch, secteur=secteur, cas=profils, qemu=run(
        ["qemu-system-"+arch, "--version"], capture_output=True, text=True, check=True).stdout.splitlines()[0],
        elf_sha256=sha(noyau), image_avant_apres_sha256=sha(image), series_sha256=sha(trace))
    (sortie/"resultats.json").write_text(json.dumps(resultat,ensure_ascii=False,indent=2)+"\n")
    return resultat


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=Path)
    p.add_argument("architecture", choices=("aarch64","x86_64"))
    p.add_argument("secteur", type=int, choices=(512,4096))
    p.add_argument("sortie", type=Path)
    a=p.parse_args()
    print(json.dumps(preuve(a.archive.resolve(),a.architecture,a.secteur,a.sortie.resolve()), ensure_ascii=False))
