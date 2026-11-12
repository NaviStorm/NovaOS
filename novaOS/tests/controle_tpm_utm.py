"""Compare les deux interfaces TPM sur le moteur d'UTM et des disques jetables."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import time

from bloc import qmp
from installation_utm import FIRMWARE, empreinte, lanceurs
from prototype_uefi import image, pflash, tpm


def mesurer(tmp, sortie, disque, artefacts, modele, nombre):
    nom = f"{modele}-{nombre}"
    serie = sortie / (nom + ".serial")
    var = tmp / (nom + ".fd")
    pflash(artefacts / "modele-vars.fd", var)
    cible = tmp / (nom + ".raw")
    if nombre == 2:
        with cible.open("xb") as f:
            f.truncate(64 << 20)
        avant = empreinte(cible)
    args = [tmp / "qemu-aarch64-softmmu", "-L", FIRMWARE, "-machine", "virt-10.0,gic-version=3,its=off",
            "-accel", "tcg", "-cpu", "cortex-a72", "-m", "256", "-smp", "1", "-nic", "none",
            "-display", "none", "-device", "ramfb", "-serial", f"file:{serie}", "-monitor", "none",
            "-qmp", "stdio", "-drive", f"if=pflash,unit=0,format=raw,readonly=on,file={tmp / 'code.fd'}",
            "-drive", f"if=pflash,unit=1,format=raw,file={var}", "-device",
            "virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off"]
    supports = [(disque, "source", "on")]
    if nombre == 2:
        supports.append((cible, "cible", "off"))
    for i, (chemin, identifiant, mode) in enumerate(supports):
        args += ["-drive", f"if=none,id={identifiant},format=raw,readonly={mode},file={chemin}",
                 "-device", f"virtio-blk-pci,drive={identifiant},addr={6+i},disable-legacy=on,"
                 f"iommu_platform=on,ats=off,num-queues=1,vectors=0,bootindex={1+i}"]
    if modele == "crb":
        args += ["-d", "int", "-D", sortie / (nom + ".exception")]
    for event in ("virtio_iommu_attach", "virtio_iommu_detach", "virtio_iommu_map", "virtio_iommu_unmap_done",
                  "virtio_iommu_report_fault", "virtio_blk_handle_read", "virtio_blk_handle_write"):
        args += ["-trace", event]
    with tpm(tmp, sortie, nom) as sock, (sortie / (nom + ".trace")).open("xb") as trace:
        args += ["-chardev", f"socket,id=tch,path={sock}", "-tpmdev", "emulator,id=tpm0,chardev=tch",
                 "-device", f"tpm-{modele}-device,tpmdev=tpm0"]
        debut = time.monotonic()
        with subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=trace, bufsize=0) as vm:
            try:
                qmp(vm, "qmp_capabilities")
                motif = None
                instants = {}
                jalons = (("uefi", b"UEFI firmware"), ("dxe", b"NOVA DXE politique active"),
                          ("chargeur", b"OBS EFI StartImage Limine"),
                          ("sortie", b"OBS EFI notification ExitBootServices"),
                          ("noyau", b"NovaOS v0.14.0"), ("lecture", b"DMA motif correct"))
                while time.monotonic() - debut < 45 and vm.poll() is None:
                    s = serie.read_bytes() if serie.exists() else b""
                    for nom_jalon, texte in jalons:
                        if nom_jalon not in instants and texte in s:
                            instants[nom_jalon] = round(time.monotonic() - debut, 3)
                    motif = next((m for m in (b"DMA bilan Ok(())", b"Synchronous Exception at 0x252C8",
                                              b"Shell>", b"DMA bilan Err(", b"NOVA DXE QUARANTAINE") if m in s), None)
                    if motif:
                        break
                    time.sleep(0.005)
                if vm.poll() is not None:
                    raise RuntimeError(f"{nom} QEMU terminé, voir la trace")
                duree = time.monotonic() - debut
                stats = {n["device"]: [n["stats"][k] for k in ("rd_operations", "wr_operations")]
                         for n in qmp(vm, "query-blockstats") if n["device"] in ("source", "cible")}
            finally:
                if vm.poll() is None:
                    vm.kill()
                vm.wait()
    resultat = dict(marqueur=motif.decode() if motif else None, secondes=round(duree, 3), jalons_s=instants,
                    operations_lecture_ecriture=stats)
    if nombre == 2:
        resultat["cible_sha256_avant"] = avant
        resultat["cible_sha256_apres"] = empreinte(cible)
    return resultat


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artefacts", type=Path)
    parser.add_argument("sortie", type=Path, help="nouveau répertoire de preuves")
    args = parser.parse_args()
    args.sortie.mkdir()
    with tempfile.TemporaryDirectory(prefix="nova-tpm-", dir="/tmp") as dossier:
        tmp = Path(dossier)
        lanceurs(tmp)
        version = subprocess.check_output([tmp / "qemu-aarch64-softmmu", "--version"], text=True).splitlines()[0]
        pflash(args.artefacts / "prototype.fd", tmp / "code.fd")
        disque = image(tmp / "temoin", args.artefacts / "novaos.elf", "dma-qual")
        bilan = dict(moteur=version, pflash_sha256=empreinte(tmp / "code.fd"),
                     noyau_sha256=empreinte(args.artefacts / "novaos.elf"),
                     source_sha256=empreinte(disque), essais={})
        for modele in ("crb", "tis"):
            for nombre in (1, 2):
                r = mesurer(tmp, args.sortie, disque, args.artefacts, modele, nombre)
                bilan["essais"][f"{modele}-{nombre}"] = r
                print(modele, nombre, r, flush=True)
        assert empreinte(disque) == bilan["source_sha256"]
        (args.sortie / "resultats.json").write_text(json.dumps(bilan, indent=2, ensure_ascii=False) + "\n")
        # Contrat de référence de bb0678d, à requalifier lors d'un changement de firmware.
        for nom, attendu in (("crb-1", "Synchronous Exception at 0x252C8"),
                             ("crb-2", "Synchronous Exception at 0x252C8"),
                             ("tis-1", "DMA bilan Ok(())"), ("tis-2", "Shell>")):
            r = bilan["essais"][nom]
            assert r["marqueur"] == attendu, (nom, r)
            stats = r["operations_lecture_ecriture"]
            assert stats["source"][1] == 0
            assert (stats["source"][0] > 0) == (nom == "tis-1")
            if nom.endswith("-2"):
                assert stats["cible"] == [0, 0]
                assert r["cible_sha256_avant"] == r["cible_sha256_apres"]


if __name__ == "__main__":
    main()
