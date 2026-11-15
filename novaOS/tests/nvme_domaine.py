"""Contrelecture hôte du domaine NVMe Q35, VM arrêtée sur son bilan."""
import hashlib
import struct
import re

from materiel_pilote import read_config
from vtd_edu import physical32, registers, require, table


def observe(vm, directory, quarantine, permission=False, serial=None):
    """Conserve les tables de cette VM jetable et vérifie leur contenu via QMP."""
    require(not permission or quarantine, "Le témoin sans feuille doit demeurer en quarantaine")
    seen = set()
    digests = {}

    def page(address, name):
        require(address not in seen, f"Le cadre {name} possède déjà un autre rôle")
        seen.add(address)
        block = table(vm, address, directory / f"nvme-vtd-{name}.bin")
        digests[name] = hashlib.sha256(block).hexdigest()
        return struct.unpack("<512Q", block)

    def only(entries, first, length=1):
        require(not any(entries[:first]) and not any(entries[first + length:]),
                "Entrée supplémentaire dans les traductions VT-d")
        return entries[first:first + length]

    require(read_config(vm, "x86_64", 0, 6, 0) == 0x00101b36, "Identité Q35 NVMe")
    command = read_config(vm, "x86_64", 4, 6, 0) & 0xffff
    require(command & 0x406 == 0x402, "BME encore ouvert ou INTx actif au retrait")
    regs = registers(vm)
    require(regs["gsts"] == 0xc4000000 and regs["iqh"] == regs["iqt"], "Remappeur ou file QI")
    tail = regs["iqt"]
    require(0 < tail < 4096 and tail % 48 == 0, "Fin de file QI hors de la campagne")
    root, = only(page(regs["root"], "racine"), 0)
    require(root & 4095 == 1, "Descripteur de racine")
    context = page(root & ~4095, "contexte")
    commands = page(regs["iqa"], "qi")
    count = tail // 48
    receipts = set()
    for index in range(count):
        cc, cc_hi, iotlb, iotlb_hi, wait, address = commands[6*index:6*index+6]
        require((cc, cc_hi, iotlb, iotlb_hi, wait) ==
                (0x11, 0, 0xd2, 0, ((index+1) << 32) | 0x25),
                "Ordre invalidation contexte, IOTLB DR/DW, WAIT")
        receipts.add(address)
    require(not any(commands[6*count:]) and len(receipts) == 1, "Descripteur inattendu")
    receipt, = receipts
    require(receipt not in seen and 4096 <= receipt < (256 << 20) and receipt % 4096 == 0,
            "Cadre du reçu non réservé")
    seen.add(receipt)
    require(physical32(vm, receipt) == count, "Absence du dernier reçu WAIT")
    leaves = []
    denied = None
    if quarantine:
        ptr, attr = only(context, 2*0x30, 2)
        require(ptr & 4095 == 1 and attr == 0x102, "Le contexte doit rester privé")
        base = 0
        for shift in (39, 30, 21):
            entries = page(ptr & ~4095, f"table-{shift}")
            slots = [i for i, entry in enumerate(entries) if entry]
            require(len(slots) == 1, "Chemin d'un autre groupe dans les tables secondaires")
            slot, = slots
            ptr = entries[slot]
            require(ptr & 4095 == 3, "Permission d'un pointeur de table")
            base |= slot << shift
        entries = page(ptr & ~4095, "table-12")
        slots = [i for i, entry in enumerate(entries) if entry]
        expected_rights = (1, 2, 1, 2) if permission else (1, 2, 1, 2, 2)
        require(len(slots) == len(expected_rights)
                and slots == list(range(slots[0], slots[0] + len(expected_rights))),
                "Nombre et position des feuilles")

        start = base + slots[0]*4096
        require(0x0200_0000 <= start < 1 << 28 and (start-0x0200_0000) % 0x10000 == 0,
                "Fenêtre IOVA d'époque")
        for slot, rights in zip(slots, expected_rights):
            entry = entries[slot]
            pa = entry & ~4095
            require(entry & 4095 == rights and 4096 <= pa < (256 << 20) and pa not in seen,
                    "Cadre client aliasé ou permission différente")
            seen.add(pa)
            leaves.append({"iova": base + slot*4096, "droits": rights})
        if permission:
            # Cette VM donne au canal de diagnostic le cadre privé de ce seul essai.
            # Recouper l'absence de PTE avec l'identité de la faute du remappeur.
            if serial is None:
                serial = (directory / "serie.log").read_text()
            match = re.search(r"NVME refus DMA SID=48 faute=5 lecture=false iova=(0x[0-9a-f]+) cadre_temoin=(0x[0-9a-f]+) pte=0 zero=true", serial)
            require(match, "Le superviseur n'a pas confirmé la faute et le témoin intact")
            iova, guarded_pa = (int(v, 16) for v in match.groups())
            require(iova == start + 4*4096, "Faute hors de la page de destination prévue")
            require(not any(page(guarded_pa, "temoin-refuse")), "Contenu de la page témoin altéré")
            fr = 0xfed90000 + ((regs["cap"] >> 24) & 1023) * 16
            high = physical32(vm, fr + 8) | physical32(vm, fr + 12) << 32
            low = physical32(vm, fr) | physical32(vm, fr + 4) << 32
            require(regs["fsts"] == 2 and low == iova and high == (1 << 63) | (5 << 32) | 0x30,
                    "FRCD inexact pour le DMA NVMe en écriture dans la page absente")
            denied = {"sid": 0x30, "raison": 5, "iova": iova, "ecriture": True,
                      "gpa_temoin": guarded_pa, "octets_verifies": 4096}
    else:
        require(not any(context) and regs["fsts"] == 0, "Retrait incomplet du contexte")
    return {"commande_pci": command, "qi_et_wait": count, "gsts": regs["gsts"],
            "fsts": regs["fsts"], "dma_refuse": denied, "cadres_clients_mappes": leaves, "sha256_tables": digests}
