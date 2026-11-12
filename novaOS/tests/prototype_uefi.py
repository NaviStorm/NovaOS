"""Banc DXE/GICv3 sur les bibliothèques livrées dans UTM 5.0.8."""
import argparse
from contextlib import contextmanager
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from bloc import qmp
from installation_utm import FIRMWARE, ROOT, UTM, empreinte, lanceurs
from prototype_traces import mesures
from uefi_phases import support_temoin


def image(tmp, noyau, scenario):
    tmp.mkdir()
    support_temoin(tmp, noyau, scenario, mapkey=scenario == "mapkey")
    esp = tmp / "esp"
    shutil.copytree(tmp / "iso/boot", esp / "boot", ignore=shutil.ignore_patterns("efi.img"))
    shutil.copyfile(tmp / "iso/limine.conf", esp / "limine.conf")
    disque = tmp / "disque.raw"
    subprocess.run(["qemu-img", "convert", "-f", "raw", "-O", "raw", f"fat:{esp}", disque], check=True)
    # Les LBA 16..23 occupent l’intervalle libre précédant la première partition MBR.
    with disque.open("r+b") as f:
        mbr = f.read(512)
        partitions = [int.from_bytes(mbr[i+8:i+12], "little") for i in range(446, 510, 16) if mbr[i+4]]
        assert mbr[510:] == b"\x55\xaa" and partitions and min(partitions) >= 24
        f.seek(8192)
        assert f.read(4096) == bytes(4096)
        f.seek(8192)
        f.write(bytes(0x71 + i % 83 for i in range(4096)))
    return disque


@contextmanager
def tpm(tmp, traces, nom, etat=None):
    with tempfile.TemporaryDirectory(dir=tmp) as dossier, (traces / (nom + ".tpm")).open("xb") as journal:
        socket = Path(dossier) / "tpm"
        with subprocess.Popen([tmp / "swtpm.0", "--tpm2", "--tpmstate", f"dir={etat or dossier}",
                               "--ctrl", f"type=unixio,path={socket}", "--terminate"],
                              stdout=journal, stderr=journal) as proc:
            try:
                limite = time.monotonic() + 5
                while not socket.exists():
                    if proc.poll() is not None or time.monotonic() > limite:
                        raise RuntimeError(f"TPM indisponible, voir {journal.name}")
                    time.sleep(0.02)
                yield socket
            finally:
                if proc.poll() is None:
                    proc.terminate()
                proc.wait(timeout=5)


def attendre(vm, serie, motif, apres=0, limite=50):
    fin = time.monotonic() + limite
    while time.monotonic() < fin and vm.poll() is None:
        contenu = serie.read_bytes()[apres:] if serie.exists() else b""
        if motif in contenu:
            return
        if any(m in contenu for m in (b"Panique du noyau", b"OBS EFI panique", b"DMA bilan Err(")) or (
                b"NOVA DXE QUARANTAINE " in contenu and not motif.startswith(b"NOVA DXE QUARANTAINE")):
            raise RuntimeError(contenu[-800:].decode(errors="replace"))
        time.sleep(0.02)
    raise RuntimeError(f"{motif!r} absent, voir {serie}")


def machine(tmp, traces, nom, code, var, disque, cd=None, slot=6):
    serie, trace = traces / (nom + ".serial"), traces / (nom + ".trace")
    with tpm(tmp, traces, nom) as socket, trace.open("xb") as log:
        carte = "virt-10.0,gic-version=3,its=off" + (",virtualization=on" if nom == "el2" else "")
        backend = f"blkdebug::{disque}" if nom == "coupure" else disque
        mode = "readonly=on" if nom == "coupure" else "snapshot=on"
        args = [tmp / "qemu-aarch64-softmmu", "-L", FIRMWARE, "-machine", carte,
                "-accel", "tcg", "-cpu", "cortex-a72", "-smp", "1", "-m", "256M", "-nic", "none",
                "-display", "none", "-device", "ramfb", "-rtc", "base=utc,clock=host", "-monitor", "none",
                "-serial", f"file:{serie}", "-qmp", "stdio",
                "-drive", f"if=pflash,unit=0,format=raw,readonly=on,file={code}",
                "-drive", f"if=pflash,unit=1,format=raw,file={var}",
                "-chardev", f"socket,id=tch,path={socket}", "-tpmdev", "emulator,id=tpm0,chardev=tch",
                "-device", "tpm-tis-device,tpmdev=tpm0",
                "-device", "virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off",
                "-drive", f"if=none,id=d,format=raw,file={backend},{mode},cache=writeback,rerror=report,werror=report",
                "-device", f"virtio-blk-pci,drive=d,addr={slot},disable-legacy=on,iommu_platform=on,ats=off,"
                           f"num-queues=1,vectors=0,bootindex={2 if cd else 1}"]
        if nom == "coupure":
            args.append("-S")
        if cd:
            args += ["-device", "virtio-scsi-pci,addr=1,disable-legacy=on,iommu_platform=on,ats=off,vectors=0",
                     "-drive", f"if=none,id=cd,file={cd},format=raw,readonly=on,media=cdrom",
                     "-device", "scsi-cd,drive=cd,bootindex=1"]
        for event in ("virtio_iommu_attach", "virtio_iommu_map", "virtio_iommu_unmap", "virtio_iommu_unmap_done",
                      "virtio_iommu_fill_resv_property", "virtio_iommu_detach",
                      "virtio_iommu_translate", "virtio_iommu_report_fault", "virtio_blk_handle_read",
                      "virtio_blk_handle_write", "gicv3_icc_iar1_read", "gicv3_icc_eoir_write"):
            args += ["-trace", event]
        with subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, bufsize=0) as vm:
            try:
                qmp(vm, "qmp_capabilities")
                if nom == "coupure":
                    def debug(commande):
                        retour = qmp(vm, "human-monitor-command", **{"command-line": f'qemu-io d "{commande}"'})
                        if retour:
                            raise RuntimeError(retour)
                    debug("break read_aio premiere_lecture")
                    qmp(vm, "cont")
                    attendre(vm, serie, b"premier MAP acquitte")
                    attendre(vm, trace, b"virtio_blk_handle_read ")
                    debug("wait_break premiere_lecture")
                    stats = next(x["stats"] for x in qmp(vm, "query-blockstats") if x["device"] == "d")
                    assert stats["rd_operations"] == 0 and stats["wr_operations"] == 0
                    vm.kill()
                    (traces / "coupure.json").write_text(json.dumps({"lecture_en_vol": True,
                        "lectures_finies": stats["rd_operations"], "ecritures_finies": stats["wr_operations"]}) + "\n")

                else:
                    marque = {"autre-slot": b"DMA bilan Err(Absent)", "mapkey": b"OBS EFI seconde sortie reussie",
                              "retrait-suspendu": "NOVA DXE QUARANTAINE délai commande IOMMU".encode()}.get(nom, b"DMA bilan Ok(())")
                    attendre(vm, serie, marque)
                    if nom == "redemarrage":
                        taille = serie.stat().st_size
                        qmp(vm, "system_reset")
                        attendre(vm, serie, marque, taille)
            finally:
                if vm.poll() is None:
                    vm.kill()
                vm.wait()
    return serie, trace


def pflash(source, dest):
    if source.stat().st_size > 64 << 20:
        raise ValueError("pflash dépasse la banque UTM")
    shutil.copyfile(source, dest)
    with dest.open("ab") as f:
        f.truncate(64 << 20)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fv", type=Path, help="répertoire EDK II contenant QEMU_EFI.fd et QEMU_VARS.fd")
    parser.add_argument("noyau", type=Path, help="ELF construit avec gicv3-lab")
    normaux = ["froid", "el2", "protection", "reemploi", "cd", "redemarrage", "coupure", "autre-slot", "mapkey"]
    parser.add_argument("--cas", nargs="+", choices=[*normaux, "retrait-suspendu"], default=normaux)
    parser.add_argument("--fv-sans-ack", type=Path, help="FV distinct de l’injection négative")
    args = parser.parse_args()
    traces = Path(tempfile.mkdtemp(prefix="prototype-dxe-", dir=ROOT / "build"))
    print(traces, flush=True)
    with tempfile.TemporaryDirectory(prefix="nova-dxe-vm-", dir="/tmp") as dossier:
        tmp = Path(dossier)
        lanceurs(tmp)
        version = subprocess.check_output([tmp / "qemu-aarch64-softmmu", "--version"], text=True).splitlines()[0]
        utm = subprocess.check_output(["plutil", "-extract", "CFBundleShortVersionString", "raw",
                                      UTM / "Info.plist"], text=True).strip()
        assert (utm, version) == ("5.0.8", "QEMU emulator version 10.0.12")
        code = tmp / "code.fd"
        pflash(args.fv / "QEMU_EFI.fd", code)
        variante = tmp / "sans-ack.fd"
        if args.fv_sans_ack:
            pflash(args.fv_sans_ack / "QEMU_EFI.fd", variante)
            assert empreinte(code) != empreinte(variante)
        cas = list(args.cas)
        if args.fv_sans_ack and "retrait-suspendu" not in cas:
            cas.append("retrait-suspendu")
        if "retrait-suspendu" in cas and not args.fv_sans_ack:
            parser.error("--fv-sans-ack requis pour l’injection")
        # Conserver les artefacts exacts du banc dans build/, y compris après la VM.
        for source, nom in ((args.fv / "QEMU_EFI.fd", "prototype.fd"), (args.fv / "QEMU_VARS.fd", "modele-vars.fd"),
                            (args.noyau, "novaos.elf")):
            shutil.copyfile(source, traces / nom)
        if args.fv_sans_ack:
            shutil.copyfile(args.fv_sans_ack / "QEMU_EFI.fd", traces / "retrait-suspendu.fd")
        bilan = dict(utm=utm, moteur=version, noyau_sha256=empreinte(args.noyau),
                     firmware_sha256=empreinte(args.fv / "QEMU_EFI.fd"), pflash_sha256=empreinte(code),
                     modele_vars_sha256=empreinte(args.fv / "QEMU_VARS.fd"),
                     pflash_sans_ack_sha256=empreinte(variante) if args.fv_sans_ack else None,
                     disques={}, essais={})
        disques = {}
        for nom in cas:
            scenario = {"protection": "dma-fence", "reemploi": "dma-reemploi", "mapkey": "mapkey"}.get(nom, "dma-qual")
            if scenario not in disques:
                disques[scenario] = image(tmp / scenario, args.noyau, scenario)
                bilan["disques"][scenario] = empreinte(disques[scenario])
            disque = disques[scenario]
            var = tmp / (nom + ".vars.fd")
            pflash(args.fv / "QEMU_VARS.fd", var)
            serie, trace = machine(tmp, traces, nom, variante if nom == "retrait-suspendu" else code, var, disque,
                                   disque.parent / "cli.iso" if nom == "cd" else None,
                                   7 if nom == "autre-slot" else 6)
            bilan["essais"][nom] = mesures(traces, nom)
            if nom == "coupure":
                machine(tmp, traces, "reprise", code, var, disque)
                bilan["essais"]["reprise"] = mesures(traces, "reprise")
            print(nom, serie, trace, flush=True)
        for scenario, disque in disques.items():
            assert empreinte(disque) == bilan["disques"][scenario]
        assert empreinte(code) == bilan["pflash_sha256"]
        if "el2" in cas and "reemploi" in cas:
            assert any(x["eoi_ppi30"] for x in bilan["essais"].values())
        (traces / "resultats.json").write_text(json.dumps(bilan, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
