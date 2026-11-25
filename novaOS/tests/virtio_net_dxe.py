"""Observation passive du NIC avec le DXE AArch64 livré et le support NovaOS du banc."""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tarfile
import tempfile
import time

from installation_utm import FIRMWARE, UTM, lanceurs
from profils_utm import disque_sequentiel
from prototype_uefi import pflash, tpm
from virtio_net_modele_prive import executable
from virtio_net_confinement import ROOT, capture, digest, dump, qmp
from virtio_net_qemu import qualification


def support(tmp, iso, elf_sha):
    esp = tmp / "esp"
    (esp / "boot").mkdir(parents=True)
    subprocess.run(["xorriso", "-osirrox", "on", "-indev", iso,
                    "-extract", "/EFI", esp / "EFI", "-extract", "/limine.conf", esp / "limine.conf",
                    "-extract", "/boot/novaos.elf", esp / "boot/novaos.elf"], check=True)
    if digest(esp / "boot/novaos.elf") != elf_sha:
        raise ValueError("ELF de l'ESP différent de l'inventaire")
    subprocess.run(["qemu-img", "convert", "-f", "raw", "-O", "raw", f"fat:{esp}", tmp / "support.raw"], check=True)


def firmware_files(tmp):
    release = "novaos-0.17.0-utm-aarch64"
    archive = ROOT.parent / "distribution/images" / (release + ".tar.gz")
    with tarfile.open(archive) as bundle:
        manifest = bundle.extractfile(release + "/SHA256SUMS").read().decode().splitlines()
        for name, bank in (("prototype.fd", "code.fd"), ("modele-vars.fd", "vars.fd")):
            source = tmp / name
            with source.open("xb") as dest:
                shutil.copyfileobj(bundle.extractfile(f"{release}/{name}"), dest)
            if f"{digest(source)}  {name}" not in manifest:
                raise ValueError(f"Firmware non conforme au paquet : {name}")
            pflash(source, tmp / bank)
    return digest(archive)


def observe(tmp, out, sock, *, disque=None, secteur=None, phase=None, event_idx=None):
    serial = out / "serie.log"
    args = [executable("aarch64", tmp / "qemu-aarch64-softmmu", out), "-L", FIRMWARE, "-machine", "virt-10.0,gic-version=3,its=off",
            "-accel", "tcg", "-cpu", "cortex-a72", "-m", "256", "-smp", "1", "-display", "none",
            "-monitor", "none", "-serial", f"file:{serial}", "-qmp", "stdio", "-S", "-no-reboot",
            "-drive", f"if=pflash,unit=0,file={tmp}/code.fd,format=raw,readonly=on",
            "-drive", f"if=pflash,unit=1,file={tmp}/vars.fd,format=raw,snapshot=on",
            "-device", "ramfb", "-chardev", f"socket,id=tch,path={sock}", "-tpmdev",
            "emulator,id=tpm0,chardev=tch", "-device", "tpm-crb-device,tpmdev=tpm0",
            "-device", "virtio-iommu-pci,id=i,addr=5,granule=4k,aw-bits=48,boot-bypass=off",
            "-drive", f"if=none,id=source,format=raw,readonly=on,file={tmp}/support.raw",
            *disque_sequentiel("source", 6, True),
            "-nic", "none", "-netdev", "hubport,id=l2,hubid=17", "-device",
            "virtio-net-pci,id=nic,addr=7,netdev=l2,iommu_platform=on,ats=off,disable-legacy=on,vectors=0,romfile="]
    termine = b"MATERIEL bilan"
    attendu = b"MATERIEL bilan ok=true"
    if disque is not None:
        if secteur not in (512, 4096):
            raise ValueError("Secteur du disque de qualification inconnu")
        debut = args.index("-drive", args.index("virtio-iommu-pci,id=i,addr=5,granule=4k,aw-bits=48,boot-bypass=off"))
        options = disque_sequentiel("source", 6, True, designation="NovaOS-System-v1", serie="NOVA-CIBLE-000000001")
        options[1] += f",logical_block_size={secteur},physical_block_size={secteur}"
        args[debut:debut+6] = ["-drive", f"if=none,id=source,format=raw,cache=writeback,file={disque}", *options]
        termine, attendu = b"NET ARM bilan", b"NET ARM bilan Ok(())"
        if phase:
            choix = {"avant-f1": b"COUPURE avant F1", "apres-f1": b"COUPURE apres F1",
                     "avant-f2": b"COUPURE avant F2", "apres-f2": b"COUPURE apres F2"}
            if phase == "remonter-ro":
                args[debut+1] += ",readonly=on"
            elif phase not in ("files-nic", "parcours-fichiers"):
                attendu = termine = choix[phase]
            args += ["-fw_cfg", f"name=opt/novaos/net-p9,string={phase}"]
    if event_idx is not None:
        if event_idx not in ("on", "off") or disque is None:
            raise ValueError("Profil EVENT_IDX inconnu")
        args.extend(["-global", "virtio-net-pci.event_idx=" + event_idx])
        if event_idx == "off":
            attendu = b"NET ARM bilan Err(Feature)"
    for event in ("pci_cfg_write", "virtio_iommu_system_reset", "virtio_iommu_switch_address_space",
                  "virtio_iommu_set_config", "virtio_iommu_attach", "virtio_iommu_detach",
                  "virtio_iommu_map", "virtio_iommu_unmap_done", "virtio_iommu_translate",
                  "virtio_iommu_report_fault"):
        args += ["-trace", event]
    dump(out / "arguments.json", list(map(str, args)))
    with (out / "qemu.trace").open("xb") as log, subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0) as vm:
        try:
            qmp(vm, "qmp_capabilities")
            dump(out / "version.json", qmp(vm, "query-version"))
            capture(vm, out, "reset", True)
            qmp(vm, "cont")
            until = time.monotonic() + (90 if disque else 40)
            while time.monotonic() < until:
                data = serial.read_bytes()
                if any(s in data for s in (termine, b"NET ARM bilan", b"NOVA DXE QUARANTAINE", b"Panique du noyau")):
                    break
                time.sleep(0.05)
            qmp(vm, "stop")
            capture(vm, out, "fin", True)
            if phase in ("avant-f1", "apres-f1", "avant-f2", "apres-f2") and attendu in data:
                vm.kill()
                vm.wait(timeout=5)
                return True
            qmp(vm, "quit")
            if vm.wait(timeout=5):
                raise RuntimeError("Sortie anormale du moteur QEMU")
        finally:
            if vm.poll() is None:
                vm.kill()
            vm.wait()
    return attendu in serial.read_bytes()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path, help="répertoire aarch64 de l'archive inventaire")
    parser.add_argument("sortie", type=Path)
    args = parser.parse_args()
    out = args.sortie.resolve()
    out.mkdir(parents=True, exist_ok=False)
    iso = args.image.resolve() / "inventaire.iso"
    expected = json.loads((iso.parent / "bilan.json").read_text())
    if digest(iso) != expected["sha256_iso"]:
        raise ValueError("ISO altérée")
    report = {"bail_attribue": False}
    try:
        with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"], prefix="net") as temp:
            tmp = Path(temp)
            lanceurs(tmp)
            report["paquet_sha256"] = firmware_files(tmp)
            support(tmp, iso, expected["sha256_elf"])
            dump(out / "empreintes-entrees.json", {p.name: digest(p) for p in [
                iso, tmp / "support.raw", tmp / "code.fd", tmp / "vars.fd",
                UTM / "Frameworks/qemu-aarch64-softmmu.framework/qemu-aarch64-softmmu"]})
            with qualification(ROOT.parent.parent, report):
                start = time.monotonic()
                try:
                    with tpm(tmp, out, "labo") as sock:
                        report["bilan_invite"] = observe(tmp, out, sock)
                finally:
                    report["execution_s"] = time.monotonic() - start
    finally:
        dump(out / "campagne.json", report)


if __name__ == "__main__":
    def interrupt(signum, frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupt)
    main()
