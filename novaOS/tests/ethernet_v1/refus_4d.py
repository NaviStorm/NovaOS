"""Oracle des refus 4D, limité à la machine ARM mono-NIC."""
import json
import re


def verifier(cas, out, serie, rapport):
    if (not rapport.get("volume_relu_identique") or rapport.get("elf_relu_esp") != rapport["elf"]
            or rapport["trames_par_sens"] or rapport["erreur_pair"]):
        raise ValueError("volume P9, copie EFI ou pair inattendu")
    if ("NET ARM bilan Err(State)" not in serie or "D4 lancement scelle" in serie
            or "ETH3 session" in serie or "Panique" in serie):
        raise ValueError("état final du lancement inattendu")
    conditions = {
        "signature": ["D4 controle interrompu marque=2", "mode=1", "NET ARM refus preparation=State cadres_dma=0"],
        "revision": ["D4 plan perime revision=1", "mode=2", "NET ARM refus preparation=State cadres_dma=0"],
        "racines": ["D4 saturation reelle racines=4", "D4 refus provisionnement epoch=1 mode=3 ipc_pages=0",
                    "NET ARM refus preparation=State cadres_dma=0"],
        "droits": ["mode=6","NET ARM refus preparation=State cadres_dma=0"],
        "attribution": ["D4 objet scelle","D4 refus attribution","D4 lancement refuse avant files",
                        "D4 echec scelle stockage lecture_flush=true cadres_dma=0"],
        "lancement": ["D4 refus lancement empreinte", "D4 lancement refuse avant files",
                      "D4 echec scelle stockage lecture_flush=true cadres_dma=0"],
    }
    if not all(texte in serie for texte in conditions[cas]) or "stockage lecture_flush=true" not in serie:
        raise ValueError(f"cause du refus ou lecture/FLUSH manquante pour {cas}")
    cycle = re.findall(r"D4 cycle avant=(\d+) minimum=(\d+) apres=(\d+) pic=(\d+) ipc=0 dma=0", serie)
    if len(cycle) != 1 or not re.search(r"RAM phase=4d_fin_cycle .*?ipc=0 utilisateur=0 processus=0 ", serie):
        raise ValueError("ressources de clôture manquantes")
    for phase in ("reset","fin"):
        regs = json.loads((out/(phase+".json")).read_text())
        if regs["vm"]["running"] or regs["boot_bypass"] or not regs["iommu_platform"] or regs["pci_command"] & 4:
            raise ValueError(f"IOMMU ou BME inattendu à {phase}")
    attaches = detaches = translations = 0
    firmware=True
    commande=0
    with (out/"qemu.trace").open() as stream:
        for line in stream:
            if line.startswith("virtio_iommu_detach domain=2 endpoint=48"):
                if not firmware or commande & 4: raise ValueError("transfert EFI avec BME actif")
                firmware=False
            if re.match(r"pci_cfg_write virtio-net-pci 00:07.0 @0x4 ", line):
                commande=int(line.split()[-1],16)
                if not firmware and commande & 4: raise ValueError("BME du NIC après le transfert EFI")
            attaches += line.startswith("virtio_iommu_attach domain=3 endpoint=56")
            detaches += line.startswith("virtio_iommu_detach domain=3 endpoint=56")
            translations += line.startswith("virtio_iommu_translate ") and "rid=56 " in line
    if firmware or translations or attaches != int(cas in ("lancement","attribution")) or attaches != detaches:
        raise ValueError("cycle DMA du refus incohérent")
    return dict(cause=cas, attaches=attaches, detaches=detaches, traductions_nic=translations,
                cadres_dma_fin=0, pages_ipc_fin=0, processus_fin=0)
