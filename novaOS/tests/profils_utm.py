"""Prépare deux VM UTM 5.0.8 jetables pour l'audit du démarrage, avant installation."""
import argparse
from pathlib import Path
import plistlib
import subprocess
import tempfile
import uuid

from prototype_uefi import image, pflash


def lecteur(nom, interface, fichier, lecture_seule):
    return dict(Identifier=nom, Interface=interface, ImageName=fichier,
                ImageType="Disk", ReadOnly=lecture_seule)


def profil(destination, artefacts, support, avec_cible):
    nom = "NovaOS-audit-" + ("deux-disques" if avec_cible else "un-disque")
    paquet = destination / (nom + ".utm")
    paquet.mkdir()
    data = paquet / "Data"
    data.mkdir()
    pflash(artefacts / "prototype.fd", data / "code.fd")
    pflash(artefacts / "modele-vars.fd", data / "variables.fd")
    subprocess.run(["qemu-img", "convert", "-f", "raw", "-O", "qcow2", support,
                    data / "source.qcow2"], check=True)
    lecteurs = [lecteur("code", "PFlash", "code.fd", True),
                lecteur("variables", "PFlash", "variables.fd", False),
                lecteur("source", "None", "source.qcow2", True)]
    arguments = ["-device", "ramfb", "-device",
                 "virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off"]
    disques = [("source", 6)]
    if avec_cible:
        subprocess.run(["qemu-img", "create", "-f", "qcow2", data / "cible.qcow2", "64M"], check=True)
        lecteurs.append(lecteur("cible", "None", "cible.qcow2", False))
        disques.append(("cible", 7))
    for ordre, (identifiant, slot) in enumerate(disques, start=1):
        arguments += ["-device", f"virtio-blk-pci,drive=drive{identifiant},addr={slot},"
                      f"disable-legacy=on,iommu_platform=on,ats=off,num-queues=1,vectors=0,bootindex={ordre}"]
    for evenement in ("virtio_iommu_attach", "virtio_iommu_detach", "virtio_iommu_unmap_done",
                      "virtio_iommu_report_fault", "virtio_blk_handle_read", "virtio_blk_handle_write"):
        arguments += ["-trace", evenement]
    config = dict(
        ConfigurationVersion=4, Backend="QEMU",
        Information=dict(Name=nom, UUID=str(uuid.uuid4()), IconCustom=False),
        System=dict(Architecture="aarch64", Target="virt-10.0", CPU="cortex-a72", CPUCount=1,
                    MemorySize=256, JITCacheSize=0, ForceMulticore=False, CPUFlagsAdd=[], CPUFlagsRemove=[]),
        QEMU=dict(DebugLog=True, UEFIBoot=True, RNGDevice=False, BalloonDevice=False, TPMDevice=True,
                  Hypervisor=False, RTCLocalTime=False, PS2Controller=False,
                  MachinePropertyOverride="gic-version=3,its=off,virtualization=off", AdditionalArguments=arguments),
        Input=dict(UsbBusSupport="Disabled", UsbSharing=False, MaximumUsbShare=0),
        Sharing=dict(DirectoryShareMode="None", DirectoryShareReadOnly=True, ClipboardSharing=False),
        Serial=[dict(Mode="Terminal", Target="Auto", Terminal=dict(Font="Menlo", FontSize=12))],
        Display=[], Drive=lecteurs, Sound=[], Network=[])
    with (paquet / "config.plist").open("xb") as fichier:
        plistlib.dump(config, fichier)
    print(paquet, flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artefacts", type=Path, help="répertoire de sortie de prototype_uefi.py")
    parser.add_argument("destination", type=Path, help="nouveau répertoire, créé exclusivement")
    args = parser.parse_args()
    args.destination.mkdir()
    with tempfile.TemporaryDirectory() as tmp:
        support = image(Path(tmp) / "temoin", args.artefacts / "novaos.elf", "dma-qual")
        for avec_cible in (False, True):
            profil(args.destination, args.artefacts, support, avec_cible)


if __name__ == "__main__":
    main()
