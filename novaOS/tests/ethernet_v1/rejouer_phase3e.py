"""Rejouer séparément les lots de phase 3E dans une archive de laboratoire vierge."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

from port_qemu import qualification

BANC = Path(__file__).resolve().parent
REFERENCE = "4c3bd8896c496636118a7f4751b6947fba72d1e3"
NOYAU = "fd3ff113fcaf39bee804eebdcedc44557f41817d"
ARM = ("reset-court", "reset-deces", "reset-revocation", "reset-incertain", "reset-absent", "reset-differe",
       "reset-invalide", "reset-sans-option", "reset-iommu-absent", "reset-iommu-differe")
REPLIS = ("court", "choix-data", "plein")
MMIO = ("mmio-absent", "mmio-invalide", "mmio-differe")


def lancer(script, *args):
    subprocess.run([sys.executable, "-B", str(BANC/script), *map(str,args)], check=True)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=lambda s: Path(s).resolve())
    p.add_argument("lot", choices=("reference", "modele", "images", "arm", "garde", "mmio", "ethernet", "hote"))
    a = p.parse_args()
    out = a.archive
    wt = Path(os.environ["NOVAOS_TMPDIR"]).resolve(strict=True).parent
    temoin = wt/"pilote-virtio-net"
    archives = wt/"archives-virtio-net"
    outils = wt/"archives-ethernet-phase3/reproduction-8bd3f9c"
    images = out/"images-qualifiees"
    if a.lot == "reference":
        lancer("performance.py","compiler",out/"images-reference","echo1514","--revision",REFERENCE)
    elif a.lot == "modele":
        lancer("modele_reset_arm.py",out/"modele-mmio")
    elif a.lot == "images":
        lancer("performance.py","compiler",images,*ARM,*REPLIS,"echo1514","echo1514-reset","--revision",NOYAU)
    elif a.lot == "arm":
        lancer("performance.py","rejouer",images,*ARM,"court","--arch","arm","--repetitions",1,
               "--cycle-vie","--sortie",out/"arm-qualification")
        lancer("performance.py","rejouer",images,"choix-data","plein","--arch","arm",
               "--repetitions",1,"--sortie",out/"arm-qualification")
    elif a.lot == "garde":
        cible = out/"image-refus"
        with qualification(wt, {}):
            lancer("port_construire_q35.py",cible,"--revision",NOYAU,"--phase3d","--cas","duo-retrait","--sonder-garde")
        lancer("port_q35_stockage.py","duo-retrait",cible/"duo-retrait.elf",out/"refus/q35",temoin,archives,
               outils/"limine-enroll",temoin/"novaFS/target/aarch64-apple-darwin/release/novafs")
    elif a.lot == "mmio":
        for cas in ("reset-court", *MMIO):
            options = ["--modele-recus",out/"modele-mmio/construction.json"] if cas in MMIO else []
            if cas != "mmio-absent":
                options.append("--trace-registres")
            lancer("port_qemu.py","arm",cas,images/"reset-court/arm.elf",out/"observations"/cas/"arm/1",
                   temoin,archives,outils/"limine-enroll",outils/"BOOTAA64.EFI","--limine-arm",outils/"limine-a72.EFI",
                   "--source-elf",NOYAU,"--cycle-vie",*options)
    elif a.lot == "ethernet":
        for n in range(1,4):
            for isa in ("arm","q35"):
                courant = "echo1514-reset" if isa=="arm" else "echo1514"
                variantes = ((out/"images-reference","echo1514","3d"),(images,courant,"3e"))
                for contenu,cas,label in (variantes if n%2 else reversed(variantes)):
                    lancer("performance.py","rejouer",contenu,cas,"--arch",isa,"--repetitions",n,
                           "--cycle-vie","--sortie",out/("echo-"+label))
    else:
        lancer("hote_retrait.py",out/"hote")


if __name__ == "__main__":
    main()
