"""Observation passive avant tout bail NIC. Aucune écriture MMIO ou PCI par le banc."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import time

from demarrage import ROOT, qmp
from virtio_net_qemu import qualification


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def dump(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def hmp(vm, command):
    return qmp(vm, "human-monitor-command", **{"command-line": command})


def reg(vm, address, *, optional=False):
    reply = hmp(vm, f"xp /1wx {address:#x}")
    if optional and reply.strip() == f"{address:x}: Cannot access memory":
        return None
    match = re.fullmatch(r"\s*([\da-f]+):\s*0x([\da-f]+)\s*", reply, re.I)
    if not match or int(match[1], 16) != address:
        raise RuntimeError(reply)
    return int(match[2], 16)


def capture(vm, directory, phase, arm):
    state = {"vm": qmp(vm, "query-status"), "pci": qmp(vm, "query-pci")}
    if state["vm"]["running"]:
        raise RuntimeError("Capture demandée sur une VM en mouvement")
    state["virtio"] = qmp(vm, "x-query-virtio")
    path = "/machine/peripheral/nic"
    state["iommu_platform"] = qmp(vm, "qom-get", path=path, property="iommu_platform")
    if arm:
        state["boot_bypass"] = qmp(vm, "qom-get", path="/machine/peripheral/i", property="boot-bypass")
    else:
        state["vtd_version"] = reg(vm, 0xfed90000)
        state["vtd_gsts"] = reg(vm, 0xfed9001c)
    # ECAM Q35 est configuré par le firmware. Un identifiant absent rend BME inconnu.
    cfg = (0x4010000000 if arm else 0xe0000000) + (7 << 15)
    state["pci_id_ecam"] = reg(vm, cfg, optional=True)
    if state["pci_id_ecam"] == 0x10411af4:
        state["pci_command"] = reg(vm, cfg + 4) & 0xffff
    (directory / f"{phase}-mtree.txt").write_text(hmp(vm, "info mtree"))
    dump(directory / f"{phase}.json", state)
    return state


def run(arch, iso, directory, firmware):
    directory.mkdir()
    arm = arch == "aarch64"
    emulator = Path(shutil.which(f"qemu-system-{arch}")).resolve(strict=True)
    code = firmware / f"edk2-{arch}-code.fd"
    variables = firmware / ("edk2-arm-vars.fd" if arm else "edk2-i386-vars.fd")
    serial = directory / "serie.log"
    args = ["sh", str(ROOT / "scripts/qemu.sh"), "-S", "-display", "none",
            "-monitor", "none", "-serial", f"file:{serial}", "-qmp", "stdio",
            "-netdev", "hubport,id=l2,hubid=17", "-device",
            "virtio-net-pci,id=nic,netdev=l2,addr=07.0,iommu_platform=on,ats=off,disable-legacy=on,vectors=0,romfile="]
    events = ["pci_cfg_write", "virtio_set_status"]
    if arm:
        args += ["-machine", "msi=off,default-bus-bypass-iommu=off",
                 "-global", "virtio-scsi-pci.disable-legacy=on",
                 "-global", "virtio-scsi-pci.iommu_platform=on", "-device",
                 "virtio-iommu-pci,id=i,addr=05.0,boot-bypass=off,granule=4k,aw-bits=48"]
        events += ["virtio_iommu_system_reset", "virtio_iommu_set_config", "virtio_iommu_attach",
                   "virtio_iommu_detach", "virtio_iommu_map", "virtio_iommu_unmap",
                   "virtio_iommu_translate", "virtio_iommu_report_fault",
                   "virtio_iommu_switch_address_space"]
    else:
        args += ["-device", "intel-iommu,id=i,intremap=on,eim=on,aw-bits=48,caching-mode=on,device-iotlb=off"]
        events += ["vtd_dmar_enable", "vtd_reg_write_gcmd", "vtd_reg_dmar_root", "vtd_ir_enable"]
    for name in events:
        args += ["-trace", name]
    env = dict(os.environ, NOVA_ARCH=arch, NOVA_ISO=str(iso), NOVA_GIC="2",
               NOVA_VCPUS="1", NOVA_RAM="256", OVMF_CODE=str(code), OVMF_VARS=str(variables))
    dump(directory / "entrees.json", {"arguments": args, "iso": str(iso), "sha256_iso": digest(iso),
         "firmware": str(code), "sha256_firmware": digest(code), "sha256_vars": digest(variables),
         "qemu": str(emulator), "sha256_qemu": digest(emulator), "traces_demandees": events})
    start = time.monotonic()
    with (directory / "qemu.trace").open("xb") as log, subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0, env=env) as vm:
        try:
            qmp(vm, "qmp_capabilities")
            dump(directory / "version.json", qmp(vm, "query-version"))
            capture(vm, directory, "reset", arm)
            qmp(vm, "cont")
            until = time.monotonic() + 20
            while time.monotonic() < until:
                if serial.exists() and b"MATERIEL bilan" in serial.read_bytes():
                    break
                time.sleep(0.05)
            qmp(vm, "stop")
            capture(vm, directory, "fin", arm)
            qmp(vm, "quit")
            if vm.wait(timeout=5):
                raise RuntimeError("Sortie anormale de QEMU")
        finally:
            if vm.poll() is None:
                vm.kill()
            vm.wait()
    return {"execution_s": time.monotonic() - start, "bilan_invite": b"MATERIEL bilan ok=true" in serial.read_bytes()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("images", type=Path, help="archive d'inventaire avec les deux ISO")
    parser.add_argument("sortie", type=Path, help="nouvelle archive durable")
    parser.add_argument("--arch", nargs="+", choices=("x86_64", "aarch64"), default=("x86_64", "aarch64"))
    args = parser.parse_args()
    directory = args.sortie.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    firmware = Path(subprocess.check_output(["brew", "--prefix", "qemu"], text=True).strip()) / "share/qemu"
    report = {"architectures": {}, "bail_attribue": False}
    try:
        with qualification(ROOT.parent.parent, report):
            for arch in args.arch:
                images = args.images.resolve() / arch
                iso = images / "inventaire.iso"
                if digest(iso) != json.loads((images / "bilan.json").read_text())["sha256_iso"]:
                    raise ValueError("ISO différente de la preuve d'inventaire")
                report["architectures"][arch] = run(arch, iso, directory / arch, firmware)
    finally:
        dump(directory / "campagne.json", report)


if __name__ == "__main__":
    def interrupt(signum, frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupt)
    main()
