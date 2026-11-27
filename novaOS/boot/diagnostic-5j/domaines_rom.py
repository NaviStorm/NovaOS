"""Deux baux de lecture de pages témoins, dans la flash RO du banc Q35."""

from pathlib import Path
import struct
from contrat import Bail


def construire(flash, root, calibration, scenario):
    code = bytearray(flash.read_bytes())
    pos = root - ((1 << 32) - len(code))
    if code[pos : pos + 4096] != bytes(4096) or code[
        pos + 4096 : pos + 12 * 4096
    ] != b"\xff" * (11 * 4096):
        raise ValueError("Réserve flash contiguë absente")
    code[pos : pos + 12 * 4096] = bytes(12 * 4096)

    def page(i):
        return root + 4096 * i

    def entree(i, index, valeur):
        struct.pack_into("<Q", code, pos + i * 4096 + index * 8, valeur)

    entree(0, 0, page(1) | 1)
    descriptions = []
    for n, nom in enumerate(("stock", "nic")):
        (rid,) = [d[1] * 8 + d[2] for d in calibration if d[3] == nom and d[0] == 0]
        did = n + 1
        l4 = 2 + n * 4
        entree(1, rid * 2, page(l4) | 1)
        entree(1, rid * 2 + 1, (did << 8) | 2)
        for i in range(l4, l4 + 3):
            entree(i, 0, page(i + 1) | 3)
        # IOVA 4 Kio pour chaque titulaire. HPA et DID distincts.
        cible = page(10 + n)
        bits = 3 if scenario == "droits" and n == 0 else 1
        entree(
            l4 + 3,
            1,
            (0x5000 if scenario == "contradiction" and n == 0 else cible) | bits,
        )
        descriptions.append((rid, did, cible))
    Path("domaines.fd").write_bytes(code)
    return descriptions


def verifier_domaines(
    base, root, descriptions, qt, registres, ids, epoch, recu, scenario, phase
):
    from contrat import Constat, verifier

    cadres = frozenset(root + i * 4096 for i in range(10))
    permis = cadres - {root + 4096} if scenario == "table-inconnue" else cadres
    pages = {adr: bytes.fromhex(qt(f"read {adr} 4096")[2:]) for adr in permis}
    Path(f"tables-{phase}.bin").write_bytes(b"".join(pages[k] for k in sorted(pages)))
    baux = [
        Bail(base, rid, did, ((4096, cible, 1),)) for rid, did, cible in descriptions
    ]
    photo = Constat(base, registres, pages, cadres, ids)
    return verifier(
        epoch,
        recu,
        [(0, 256 << 20)],
        [photo],
        baux,
        ids,
        [],
        scenario != "proprietaire",
        scenario == "en-vol",
    )
