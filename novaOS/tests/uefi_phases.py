"""Audit d'arbitrage A. Les images et les états TPM sont privés à chaque exécution."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from installation_utm import BANNER, FIRMWARE, ROOT, empreinte, essai, fixture_p9, lanceurs


def support_temoin(tmp, noyau=None, scenario="cli", *, mapkey=False):
    manifest = ROOT / "tests/uefi_temoin/Cargo.toml"
    cible = "aarch64-unknown-uefi"
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
    subprocess.run(["rustup", "run", "stable", "cargo", "build", "--offline", "--locked",
                    "--release", *(["--features", "mapkey-lab"] if mapkey else []),
                    "--target", cible, "--manifest-path", manifest, "--target-dir", tmp / "cargo"],
                   env={**os.environ, "RUSTC": rustc}, check=True)
    racine = tmp / "iso"
    efi = tmp / "esp/EFI/BOOT"
    efi.mkdir(parents=True)
    limine = ROOT / "third_party/limine"
    subprocess.run(["shasum", "-a", "256", "-c", "SHA256SUMS"], cwd=limine, check=True)
    shutil.copyfile(limine / "BOOTAA64.EFI", efi / "NOVALDR.EFI")
    shutil.copyfile(tmp / "cargo" / cible / "release/temoin-efi.efi", efi / "BOOTAA64.EFI")
    (racine / "boot").mkdir(parents=True)
    shutil.copytree(efi.parent, racine / "EFI")
    subprocess.run(["qemu-img", "convert", "-f", "raw", "-O", "raw", f"fat:floppy:{efi.parents[1]}",
                    racine / "boot/efi.img"], check=True)
    shutil.copyfile(noyau or ROOT / "target/aarch64-unknown-none-softfloat/release/novaos",
                    racine / "boot/novaos.elf")
    (racine / "limine.conf").write_bytes((ROOT / "boot/limine.conf").read_bytes() + f"    cmdline: {scenario}\n".encode())
    subprocess.run(["xorriso", "-as", "mkisofs", "-R", "-J", "-V", "NOVAOBS", "-e", "boot/efi.img",
                    "-no-emul-boot", "-efi-boot-part", "--efi-boot-image", "--protective-msdos-label",
                    "-o", tmp / "cli.iso", racine], check=True)


def routage(moteur, traces, version):
    dtb = traces / f"gic{version}.dtb"
    subprocess.run([moteur, "-machine", f"virt-10.0,gic-version={version},its=off,dumpdtb={dtb}",
                    "-accel", "tcg", "-cpu", "cortex-a72", "-smp", "1", "-m", "256M",
                    "-nic", "none", "-monitor", "none", "-display", "none", "-device",
                    "virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off"], check=True)

    def mots(noeud, propriete):
        sortie = subprocess.check_output(["fdtget", "-t", "x", dtb, noeud, propriete], text=True)
        return [int(s, 16) for s in sortie.split()]

    pci = "/pcie@10000000"
    masque = mots(pci, "interrupt-map-mask")
    assert masque == [0x1800, 0, 0, 7]
    table = mots(pci, "interrupt-map")
    gic = mots("/", "interrupt-parent")[0]
    # Quatre cellules PCI, phandle, deux cellules d'adresse GIC et trois d'interruption.
    assert len(table) % 10 == 0
    cle = [(6 << 11) & masque[0], 0, 0, 1]
    routes = [table[i + 4:i + 10] for i in range(0, len(table), 10) if table[i:i + 4] == cle]
    assert routes == [[gic, 0, 0, 0, 5, 4]]
    proprietes = subprocess.check_output(["fdtget", "-p", dtb, pci], text=True).splitlines()
    assert ("msi-map" in proprietes) == (version == 2)
    cadre = None
    if version == 2:
        noeud = "/intc@8000000/v2m@8020000"
        cadre = mots(noeud, "reg")
        assert cadre == [0, 0x8020000, 0, 4096]
        assert mots(pci, "msi-map") == [0, mots(noeud, "phandle")[0], 0, 65536]
    else:
        intc = "/intc@8000000"
        assert subprocess.check_output(["fdtget", dtb, intc, "compatible"], text=True).strip() == "arm,gic-v3"
        enfants = subprocess.check_output(["fdtget", "-l", dtb, intc], text=True).splitlines()
        assert not any(n.startswith(("v2m@", "its@")) for n in enfants)
        assert mots(intc, "reg") == [0, 0x8000000, 0, 0x10000, 0, 0x80a0000, 0, 0xf60000]
        assert mots("/timer", "interrupts") == [1, 13, 4, 1, 14, 4, 1, 11, 4, 1, 10, 4]
        # Les entrées iommu-map excluent le contrôleur (RID 40) de sa propre traduction.
        noeuds = subprocess.check_output(["fdtget", "-l", dtb, pci], text=True).splitlines()
        controleur, = [n for n in noeuds if n.startswith("virtio_iommu@")]
        iommu = mots(f"{pci}/{controleur}", "phandle")[0]
        assert mots(pci, "iommu-map") == [0, iommu, 0, 40, 41, iommu, 41, 65536 - 41]
    return dict(gic=version, intid=32 + routes[0][4], cadre_v2m=cadre, msi_map="msi-map" in proprietes)


def phases(profil, journal):
    data = journal.read_bytes()
    evenements = {nom: data.count(f"virtio_{nom} ".encode()) for nom in
                  ("iommu_attach", "iommu_map", "iommu_unmap", "iommu_detach", "blk_handle_read", "blk_handle_write")}

    def ordre(*marques):
        positions = [data.index(m) for m in marques]
        assert positions == sorted(positions), (profil, marques)

    if profil.endswith("-cd"):
        ordre(b"OBS EFI IoMmu absent", b"OBS EFI StartImage Limine", b"OBS EFI notification ExitBootServices", BANNER)
        if profil == "utm-cd":
            ordre(BANNER, b"virtio_iommu_fill_resv_property dev=", b"BLOC ouverture refusee Route")
            assert not any(evenements.values())
        else:
            ordre(BANNER, b"virtio_iommu_attach ", b"virtio_iommu_map ", b"virtio_blk_handle_read ",
                  b"nova> ", b"virtio_iommu_unmap ", b"virtio_iommu_detach ", b"BLOC session terminee ok=true")
            assert evenements["iommu_map"] == evenements["iommu_unmap"] == 7
            assert evenements["iommu_attach"] == evenements["iommu_detach"] == 1
            assert b"virtio_iommu_attach domain=1 endpoint=48" in data
        return dict(evenements, notification_sortie=True)
    faute = re.search(rb"virtio_iommu_translate sid=(\d+) is not known", data)
    assert faute and int(faute[1]) == (8 if profil == "utm-optique-protegee" else 48)
    sid = faute[1]
    premiere = re.search(rb"virtio_iommu_translate [^\n]*rid=" + sid + rb" addr=(0x[0-9a-f]+) flag=(\d+)", data)
    assert premiere and premiere.start() < faute.start() and b"val 15\n" in data[:premiere.start()]
    assert b"OBS EFI " not in data and BANNER not in data and not any(evenements.values())
    return dict(evenements, notification_sortie=False, sid_refuse=int(sid),
                premiere_iova=premiere[1].decode(), premiere_direction=int(premiere[2]))


def main():
    with tempfile.TemporaryDirectory(prefix="nova-ph-", dir="/tmp") as temp:
        tmp = Path(temp)
        (ROOT / "build").mkdir(exist_ok=True)
        traces = Path(tempfile.mkdtemp(prefix="arbitrage-a-", dir=ROOT / "build"))
        print(f"Journaux : {traces}", flush=True)
        lanceurs(tmp)
        moteur = tmp / "qemu-aarch64-softmmu"
        versions = [subprocess.check_output([exe, "--version"], text=True).splitlines()[0]
                    for exe in (moteur, "qemu-system-aarch64")]
        topologies = [routage(moteur, traces, version) for version in (2, 3)]
        support_temoin(tmp)
        fixture_p9(tmp / "volume.raw", 512)
        subprocess.run(["qemu-img", "convert", "-f", "qcow2", "-O", "raw",
                        FIRMWARE / "edk2-arm-vars.fd", tmp / "vars.fd"], check=True)
        supports = [tmp / "cli.iso", tmp / "volume.raw", tmp / "vars.fd", FIRMWARE / "edk2-aarch64-code.fd"]
        empreintes = {p.name: empreinte(p) for p in supports}
        essais = []
        for profil in ("utm-cd", "utm-optique-protegee", "utm-disque-protege", "utm-gic3-disque-protege",
                       "qemu-cd", "qemu-disque-protege"):
            bilan = essai(tmp, traces, profil, tracer_dma=True)
            bilan.update(phases(profil, traces / f"{profil}.serial"))
            if not bilan["notification_sortie"]:
                assert bilan["secondes"] >= 30
            print(json.dumps(bilan, ensure_ascii=False), flush=True)
            essais.append(bilan)
        assert empreintes == {p.name: empreinte(p) for p in supports}
        (traces / "resultats.json").write_text(json.dumps(dict(moteurs=versions, topologies=topologies,
                    empreintes=empreintes, essais=essais), indent=2) + "\n")


if __name__ == "__main__":
    main()
