"""Oracle indépendant de la panne après DMA, sur la VM arrêtée et le modèle de faute identifié."""
import json
import os
import re
import struct
from pathlib import Path

from virtio_net_confinement import digest, dump, qmp
from virtio_net_pair import trame


def prerequis(arch, elf):
    if os.environ.get("NOVAOS_NET_MODELE_FAUTE") is None:
        return
    if os.environ.get("NOVAOS_NET_MODELE_FAUTE") != "injecter" or os.environ.get("NOVAOS_NET_FAUTE_RX") != "completion-15":
        raise ValueError("Bail expérimental refusé avant requalification")
    cle = {"ARM": "NOVAOS_QEMU_RX_ARM", "Q35": "NOVAOS_QEMU_RX_Q35"}[arch]
    construction = Path(os.environ[cle]).resolve(strict=True)
    racine = construction.parents[2]
    modele = json.loads(construction.read_text())
    attendus = (["arm-ferme-512", "arm-ferme-4096", "arm-sans-event"] if arch=="ARM" else
                ["q35-firmwares", "q35-signature", "q35-avant-dbx", "q35-dbx", "q35-sans-iommu",
                 "q35-ferme", "q35-sans-event", "q35-hash"])
    for nom in attendus:
        dossier = racine / nom
        resultat = json.loads((dossier / "campagne.json").read_text())
        if "resultats" in resultat:
            attendu = {"variante"} if nom in ("q35-avant-dbx", "q35-sans-iommu") else {"variante", "amont"}
            if set(resultat["resultats"]) != attendu:
                raise ValueError("Contrôles du firmware incomplets pour " + nom)
            rapports = [dossier / ident / "modele-rx.json" for ident in attendu]
        else:
            if resultat.get("succes", resultat.get("bilan")) is not True:
                raise ValueError("Contrôle fermé incomplet pour " + nom)
            rapports = [dossier / "modele-rx.json"]
        for p in rapports:
            r = json.loads(p.read_text())
            if r["sha256"] != modele["sha256"] or r["construction_sha256"] != digest(construction) or r["mode_faute"] != "revue":
                raise ValueError("Contrôle fermé d’un autre exécutable")
    nom = "arm-ferme-512" if arch=="ARM" else "q35-ferme"
    p = racine / nom
    attendu = (json.loads((p/"campagne.json").read_text())["elf_sha256"] if arch=="ARM"
               else json.loads((p/"entrees.json").read_text())["novaos.elf"])
    if digest(elf) != attendu:
        raise ValueError("Invité distinct du témoin fermé")


def constater(vm, out, arch):
    if qmp(vm, "query-status")["running"]:
        raise ValueError("La capture requiert une VM arrêtée")
    serial = (out/"serie.log").read_text()
    etat = json.loads((out/"fin.json").read_text())
    if etat["pci_command"] & 4 or "NET L2 pilote fin=Exit(8) racines=0 ipc_avant=0 ipc_apres=0" not in serial:
        raise ValueError("Désarmement ou restitution du pilote absent")
    if arch=="ARM":
        if ("NET ARM bilan Err(Quarantine)" not in serial or
                "NET ARM faute disque_P9_relu=true disque_ferme=true cadres_avant=38 cadres_apres=31" not in serial):
            raise ValueError("Quarantaine ou restitution indépendante du stockage non confirmée")
    else:
        if 'NET Q35 L2 quarantaine=Quarantine' not in serial or 'NET Q35 etat=Err("trafic non confirme") bail=false' not in serial:
            raise ValueError("Statut Q35 incohérent")
    adresses = {}
    marqueurs = 0
    with (out/"qemu.trace").open() as f:
        for line in f:
            if line.startswith("NOVAOS RX DMA"):
                if line.strip()!="NOVAOS RX DMA bytes=1526 head=15 used=omitted seq=15":
                    raise ValueError("Injection différente du modèle de panne approuvé")
                marqueurs += 1
            if arch=="ARM":
                m=re.fullmatch(r"virtio_iommu_map domain=3 virt_start=(0x\w+) virt_end=0x\w+ phys_start=(0x\w+) flags=(\d+)\s*",line)
                if m:
                    i=(int(m[1],16)-0x10000000)//4096
                    if i in (2,5,13) and m[3]=="2": adresses[i]=int(m[2],16)
                if line.startswith("virtio_iommu_detach domain=3 "): adresses.clear()
            else:
                m=re.fullmatch(r"vtd_iotlb_page_(?:update|hit) .*?sid 0x38 iova (0x\w+) slpte (0x\w+) domain 0x3\s*",line)
                if m:
                    i=(int(m[1],16)-0x10000000)//4096
                    v=int(m[2],16)
                    if i in (2,5,13) and v&4095==2: adresses[i]=v&~4095
    if marqueurs!=1 or set(adresses)!={2,5,13} or len(set(adresses.values()))!=3:
        raise ValueError("Identification univoque des trois pages DMA ou du défaut impossible")
    bas = 0x40000000 if arch=="ARM" else 0
    if any(pa%4096 or not bas<=pa<bas+0x10000000 for pa in adresses.values()):
        raise ValueError("Traduction hors du RAM invité qualifié")
    pages={}
    for i,pa in adresses.items():
        fichier=out/f"nic-page-{i}.bin"
        qmp(vm,"pmemsave",val=pa,size=4096,filename=str(fichier.resolve()))
        pages[i]=fichier.read_bytes()
        if len(pages[i])!=4096:
            raise ValueError("Page mémoire incomplète")
    attendu = bytes(10) + b"\x01\x00" + trame(15,True)
    if pages[13][2048:2048+len(attendu)]!=attendu:
        raise ValueError("Copie DMA de la trame 15 différente dans le buffer RX")
    for i, compte in ((2,15),(5,16)):
        donnees = pages[i]
        if struct.unpack_from("<H",donnees,2)[0]!=compte:
            raise ValueError("Indice matériel de l’anneau utilisé inattendu")
        entrees=[struct.unpack_from("<II",donnees,4+j*8) for j in range(compte)]
        if [n for n,_ in entrees]!=list(range(compte)):
            raise ValueError("Complétion étrangère ou multiple")
    dump(out/"panne-apres-dma.json", dict(trame=15, octets_dma=len(attendu), rx_utilises=15, tx_utilises=16,
         bus_master=False, adresses_physiques=adresses, sortie_pilote=8, confirmations_modele=marqueurs,
         statut_natif=re.findall(r"NET L2 refus seq=\d+ tag=\d+ statut=(\d+)",serial),
         pages_sha256={i:digest(out/f"nic-page-{i}.bin") for i in pages}))
