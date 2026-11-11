"""Qualification sur disque unique avec les deux firmwares AArch64 livrés par UTM."""
import json
import lzma
import re
import subprocess
import tempfile
import time
from pathlib import Path
from uuid import UUID

from bloc import qmp
from installation_utm import FIRMWARE, ROOT, UTM, empreinte, essai, lanceurs
from uefi_phases import phases, routage, support_temoin


def modules_identifiables(code):
    # Les deux images UTM ont un FV DXE encapsulé dans une section GUID LZMA.
    # Les chemins CodeView identifient des modules, sans attester leur révision.
    rom = code.read_bytes()
    guid = UUID("ee4e5898-3914-4259-9d6e-dc7bd79403cf").bytes_le
    sections = [m.start() - 4 for m in re.finditer(re.escape(guid), rom)
                if m.start() >= 4 and rom[m.start() - 1] == 2]
    debut, = sections
    taille = int.from_bytes(rom[debut:debut + 3], "little")
    offset = int.from_bytes(rom[debut + 20:debut + 22], "little")
    assert 24 <= offset < taille <= len(rom) - debut
    volume = lzma.LZMADecompressor(format=lzma.FORMAT_ALONE).decompress(
        rom[debut + offset:debut + taille], max_length=16 * 1024 * 1024)
    noms = sorted({s.decode() for s in re.findall(rb"DEBUG/([A-Za-z0-9_.-]+)\.dll", volume)})
    assert {"PciBusDxe", "PciHostBridgeDxe", "Virtio10", "VirtioBlkDxe", "ArmGicDxe"} <= set(noms)
    return noms


def interruptions_el1(tmp, traces, moteur):
    # Le chargeur de QEMU place uniquement la sonde en RAM. Aucun appel EFI.
    src = ROOT / "tests/gic3_el1"
    rustc = Path(subprocess.check_output(
        ["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip())
    linker = rustc.parents[1] / "lib/rustlib/aarch64-apple-darwin/bin/rust-lld"
    objets = []
    for source in ("entree.S", "sonde.c"):
        objet = tmp / (source + ".o")
        subprocess.run(["clang", "-target", "aarch64-none-elf", "-ffreestanding",
                        "-mgeneral-regs-only", "-O2", "-Wall", "-Werror", "-c", src / source,
                        "-o", objet], check=True)
        objets.append(objet)
    sonde = tmp / "gic3.elf"
    subprocess.run([linker, "-flavor", "gnu", "-Ttext=0x40100000", "-e", "_start",
                    "-o", sonde, *objets], check=True)
    disque = tmp / "config.raw"
    with disque.open("xb") as fichier:
        fichier.truncate(1024 * 1024)
    serie = traces / "el1.serial"
    evenements = traces / "el1.trace"
    cmd = [moteur, "-machine", "virt-10.0,gic-version=3,its=off", "-accel", "tcg",
           "-cpu", "cortex-a72", "-smp", "1", "-m", "256M", "-nodefaults", "-nic", "none",
           "-display", "none", "-monitor", "none", "-serial", f"file:{serie}", "-qmp", "stdio",
           "-device", f"loader,file={sonde},cpu-num=0", "-device",
           "virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off",
           "-drive", f"if=none,id=d,file={disque},format=raw", "-device",
           "virtio-blk-pci,drive=d,addr=6,disable-legacy=on,iommu_platform=on,ats=off,vectors=0,num-queues=1"]
    for nom in ("gicv3_dist_set_irq", "gicv3_icc_iar1_read", "gicv3_icc_eoir_write",
                "virtio_iommu_translate", "virtio_iommu_attach", "virtio_iommu_map",
                "virtio_blk_handle_read", "virtio_blk_handle_write"):
        cmd += ["-trace", nom]
    with evenements.open("xb") as log, subprocess.Popen(
        cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0
    ) as vm:
        try:
            qmp(vm, "qmp_capabilities")
            for marque in ("PPI 30 termine", "INTx 37 termine et masque"):
                fin = time.monotonic() + 10
                while time.monotonic() < fin and vm.poll() is None:
                    texte = serie.read_text()
                    if "ECHEC" in texte:
                        raise RuntimeError(f"Échec invité, voir {serie}")
                    if marque + "\n" in texte:
                        break
                    time.sleep(0.02)
                else:
                    raise RuntimeError(f"Marqueur {marque} absent, voir {evenements}")
                if marque == "PPI 30 termine":
                    qmp(vm, "block_resize", device="d", size=2 * 1024 * 1024)
        finally:
            if vm.poll() is None:
                vm.kill()
            vm.wait()
    texte = evenements.read_text()
    acquittements = re.findall(r"gicv3_icc_(iar1_read|eoir_write) .* value (0x\w+)", texte)
    assert acquittements == [("iar1_read", "0x1e"), ("eoir_write", "0x1e"),
                             ("iar1_read", "0x25"), ("eoir_write", "0x25")], acquittements
    niveaux = re.findall(r"gicv3_dist_set_irq .* interrupt 37 level changed to ([01])", texte)
    assert niveaux == ["1", "0"], niveaux
    assert not re.search(r"virtio_(iommu_(translate|attach|map)|blk_handle_(read|write)) ", texte)
    return dict(timer=30, intx_configuration=37, acces_bloc=0, dma_disque=0)


def main():
    with tempfile.TemporaryDirectory(prefix="nova-gic-", dir="/tmp") as dossier:
        tmp = Path(dossier)
        (ROOT / "build").mkdir(exist_ok=True)
        traces = Path(tempfile.mkdtemp(prefix="qualification-gic3-", dir=ROOT / "build"))
        print(traces, flush=True)
        lanceurs(tmp)
        moteur = tmp / "qemu-aarch64-softmmu"
        utm = subprocess.check_output(["plutil", "-extract", "CFBundleShortVersionString", "raw",
                                       UTM / "Info.plist"], text=True).strip()
        version = subprocess.check_output([moteur, "--version"], text=True).splitlines()[0]
        assert utm == "5.0.8" and version == "QEMU emulator version 10.0.12"
        topo = routage(moteur, traces, 3)
        irq = interruptions_el1(tmp, traces, moteur)
        support_temoin(tmp)
        disque = tmp / "cli.iso"
        initial = empreinte(disque)
        essais = []
        for nom, suffixe in (("standard", ""), ("secure", "-secure"),
                             ("standard-el2", ""), ("secure-el2", "-secure")):
            profil = f"utm-gic3-{nom}-disque-protege"
            code = FIRMWARE / f"edk2-aarch64{suffixe}-code.fd"
            modele = FIRMWARE / f"edk2-arm{suffixe}-vars.fd"
            variables = tmp / f"{nom}.fd"
            subprocess.run(["qemu-img", "convert", "-f", "qcow2", "-O", "raw", modele, variables],
                           check=True)
            sources = (code, modele, variables)
            avant = [empreinte(p) for p in sources]
            resultat = essai(tmp, traces, profil, tracer_dma=True, code_efi=code,
                             variables=variables, traces_gic=True)
            journal = traces / f"{profil}.serial"
            resultat.update(phases(profil, journal))
            texte = journal.read_text(errors="replace")
            for action in ("iar1_read", "eoir_write"):
                ids = re.findall(rf"gicv3_icc_{action} .* value (0x\w+)", texte)
                assert ids and "0x25" not in ids
                resultat[action] = sorted({int(i, 16) for i in ids})
            assert avant == [empreinte(p) for p in sources] and initial == empreinte(disque)
            resultat.update(code_sha256=avant[0], modele_vars_sha256=avant[1])
            resultat["modules_identifiables"] = modules_identifiables(code)
            essais.append(resultat)
            print(json.dumps(resultat), flush=True)
        (traces / "resultats.json").write_text(json.dumps(dict(
            utm=utm, moteur=version, topologie=topo, el1=irq, disque_sha256=initial, essais=essais
        ), indent=2) + "\n")


if __name__ == "__main__":
    main()
