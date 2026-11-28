"""Registre de pages du seul domaine xHCI, limité au modèle USB synchrone Q35."""

import hashlib
import json
import re

FIRMWARE = "b2c10fe2d90f8324c350f655a90d0b5bb716cacb636cf017e55e2173b5009f5d"
PAGE = 4096
# Nom, droits VT-d, octets maximum par allocation, admissions maximum par époque.
ROLES = {
    1: ("DCBAA", 1, 2048, 1),
    2: ("événements", 2, 8192, 1),
    3: ("ERST", 1, 256, 1),
    4: ("commandes/transferts", 1, 4096, 4),
    5: ("contexte entrée", 1, 4096, 1),
    6: ("contexte sortie", 3, 4096, 1),
    7: ("USB IN", 2, 65536, 8),
    8: ("USB OUT", 1, 4096, 1),
}


def exiger(ok, motif):
    if not ok:
        raise ValueError(motif)


def canon(obj):
    return json.dumps(
        obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode()


def empreinte(b):
    return hashlib.sha256(b).hexdigest()


class Registre:
    def __init__(self, reserve, rid, epoch):
        exiger(
            reserve >= 1 << 32
            and reserve % PAGE == 0
            and 0 < rid < 256
            and isinstance(epoch, str)
            and re.fullmatch(r"[0-9a-f]{32}", epoch)
            and int(epoch, 16) > 0,
            "Identité ou réserve",
        )
        self.reserve, self.rid, self.epoch = reserve, rid, epoch
        self.loans = []
        self.structure_page = 32
        self.closed = False

    def proposer(self, seq, role, length):
        exiger(
            not self.closed
            and type(seq) is int
            and type(role) is int
            and type(length) is int
            and role in ROLES
            and seq == len(self.loans) + 1,
            "Rôle inconnu ou séquence périmée",
        )
        name, rights, maximum, limit = ROLES[role]
        exiger(
            0 < length <= maximum
            and sum(x["role"] == role for x in self.loans) < limit,
            "Quota USB",
        )
        pages = (length + PAGE - 1) // PAGE
        # Les deux tampons synchrones grandissent sans abandonner leur ancien cadre.
        # Réserver une plage d'adresses ne publie aucune traduction.
        if role in (7, 8):
            used = sum(l["pages"] for l in self.loans if l["role"] == role)
            limit_pages = 16 if role == 7 else 1
            exiger(
                length % PAGE == 0 and used + pages <= limit_pages,
                "Extension de tampon hors domaine",
            )
            first = (64 if role == 7 else 80) + used
        else:
            first = self.structure_page
            exiger(first + pages <= 64, "Réserve des structures xHCI épuisée")
        return dict(
            seq=seq,
            role=role,
            usage=name,
            droits=rights,
            octets=length,
            pages=pages,
            gpa=self.reserve + first * PAGE,
            iova=first * PAGE,
        )

    def confirmer(self, loan):
        exiger(
            loan == self.proposer(loan["seq"], loan["role"], loan["octets"]),
            "Mapping contradictoire",
        )
        self.loans.append(loan.copy())
        if loan["role"] not in (7, 8):
            self.structure_page += loan["pages"]

    def feuilles(self):
        leaves = [
            dict(
                iova=l["iova"] + i * PAGE, cadre=l["gpa"] + i * PAGE, droits=l["droits"]
            )
            for l in self.loans
            for i in range(l["pages"])
        ]

        return sorted(leaves, key=lambda f: f["iova"])

    def clore(self, submitted, completed, incarnation):
        exiger(
            not self.closed
            and submitted > 0
            and submitted == completed
            and incarnation == 1,
            "Prêt incertain ou incarnation perdue",
        )
        exiger(
            {l["role"] for l in self.loans} == set(ROLES), "Structures USB incomplètes"
        )
        self.closed = True


def source_usb(nodes, rid, part):
    exiger(
        len(nodes) == 4
        and all(isinstance(n, list) and n for n in nodes)
        and len(nodes[0]) == 3
        and nodes[0][0] == "acpi"
        and nodes[0][2] == 0
        and len(nodes[2]) == 3
        and type(nodes[2][1]) is int
        and 1 <= nodes[2][1] <= 255
        and nodes[1] == ["pci", rid >> 3, rid & 7]
        and nodes[2][0] == "usb"
        and nodes[2][2] == 0
        and nodes[3] == part,
        "Origine USB non attribuée",
    )
    return nodes[2][1]
