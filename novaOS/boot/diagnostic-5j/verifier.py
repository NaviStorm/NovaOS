"""Contre-lecture hors VM du lot publié. Aucun fichier n'est extrait sur disque."""

from hashlib import sha256
import json
from pathlib import Path
import re
import tarfile
import struct
import uuid
from contrat import Bail, Constat, verifier


def controle(ok, texte):
    if not ok:
        raise ValueError(texte)


def main():
    base = Path(__file__).resolve().parents[2] / "docs/validation/diagnostic-uefi-5j"
    for ligne in (base / "SHA256SUMS").read_text().splitlines():
        h, nom = ligne.split()
        controle(sha256((base / nom).read_bytes()).hexdigest() == h, nom)
    with tarfile.open(base / "observations.tar.gz") as archive:

        def lire(nom):
            return archive.extractfile(nom).read()

        cas = json.loads(lire("campagne.json"))
        attendus = {
            "nominal-absente",
            "nominal-inactive",
            "nominal-droits",
            "nominal-contradiction",
            "nominal-proprietaire",
            "nominal-table-inconnue",
            "nominal-en-vol",
            "nominal-active",
            "inactif-active",
            "nominal-paquet",
            "inactif-paquet",
            "revoque-active",
        }
        controle(
            set(cas) == attendus and len({c["epoque"] for c in cas.values()}) == 12,
            "Époques",
        )
        for nom, row in cas.items():
            efi = lire(nom + "/efi.log").decode()
            trace = lire(nom + "/vtd.log").decode()
            blocs = lire(nom + "/compteurs-qmp.jsonl").splitlines()
            controle(bool(blocs), "Compteurs absents")
            for ligne in blocs:
                (disque,) = (
                    s for s in json.loads(ligne) if s.get("device") == "interne"
                )
                controle(
                    all(
                        n == 0
                        for k, n in disque["stats"].items()
                        if k.endswith(("_bytes", "_operations"))
                    ),
                    "E/S internes",
                )
            if row["decision"] in (
                "quarantaine avant chargement",
                "refus avant chargement",
            ):
                controle(not efi, "Chargement après refus")
                continue
            tables = lire(nom + "/tables-avant.bin")
            controle(
                len(tables) == 40960 and tables == lire(nom + "/tables-apres.bin"),
                "Tables brutes",
            )
            epoque = uuid.UUID(row["epoque"]).bytes
            controle(row["vtd_avant"] == row["vtd_apres"], "Registres VT-d")
            regs = row["vtd_avant"]
            root = row["racine"]
            table = lire(nom + "/dmar-preliaison.bin")
            base_unite = struct.unpack_from("<Q", table, 56)[0]
            ids = frozenset(
                bus * 256 + slot * 8 + fn for bus, slot, fn, _ in row["inventaire"]
            )
            pages = {
                root + i: tables[i : i + 4096] for i in range(0, len(tables), 4096)
            }
            photo = Constat(base_unite, regs, pages, frozenset(pages), ids)
            baux = [
                Bail(base_unite, sid, did, ((4096, frame, 1),))
                for sid, did, frame in row["baux"]
            ]
            controle(
                verifier(
                    epoque,
                    epoque,
                    [(0, 256 << 20)],
                    [photo],
                    baux,
                    ids,
                    [],
                    True,
                    False,
                )
                == 40960,
                "Baux et exclusion des plages de RAM",
            )

            controle(
                lire(nom + "/epoque.bin") == b"NOVA5J-BOOT-EPOC" + epoque,
                "Époque source",
            )
            controle(
                row["tables_octets"] == 40960 and row["canari"] == 0x51A72CE3,
                "Ressources",
            )
            (rid,) = [
                slot * 8 + fn
                for _, slot, fn, qdev in row["inventaire"]
                if qdev == "sonde"
            ]
            controle(
                re.search(
                    rf"^vtd_dmar_fault sid {rid:#x} fault \d+ addr 0x5000 write 1$",
                    trace,
                    re.M,
                ),
                "Faute attribuée",
            )
            controle(
                re.findall(r"^vtd_dmar_enable .*", trace, re.M)
                == ["vtd_dmar_enable enable 1"],
                "Continuité de la traduction",
            )
            controle(trace.count("vtd_reg_dmar_root addr") == 1, "Racine modifiée")
            if nom == "revoque-active":
                controle(
                    "LoadImage=800000000000000f" in efi and "transfert=" not in efi,
                    "dbx",
                )
            else:
                marque, adresse, tsc, statut, confiance, pic, vivants, echo = (
                    struct.unpack("<16s6Q16s", lire(nom + "/recu.bin"))
                )
                controle(
                    marque == b"NOVA5J-RAM-FINAL"
                    and row["plage_image"][0] <= adresse < row["plage_image"][1]
                    and (statut, confiance, pic, vivants, echo.hex())
                    == (
                        row["statut"],
                        row["confiance"],
                        row["temporaire"],
                        row["vivants"],
                        row["echo"],
                    ),
                    "Reçu RAM brut",
                )
                controle(
                    row["confiance"] == int(nom.startswith("nominal"))
                    and not row["vivants"],
                    "Confiance et allocations",
                )
                if nom.endswith("paquet"):
                    controle(
                        row["statut"] == 14 and row["echo"] == bytes(16).hex(),
                        "Catalogue",
                    )
                else:
                    controle(
                        row["statut"] == 0 and row["echo"] == epoque.hex(), "Reçu EFI"
                    )
                    controle(
                        row["irq_depuis_pe_et_eoi"]
                        and all(e[0] == 32 for e in row["irq_depuis_pe_et_eoi"]),
                        "IRQ observées",
                    )
        ancien = json.loads(lire("heritage.json"))
        controle(
            ancien["feuilles"] == [6, 22]
            and ancien["octets"] == dict(avant=40960, apres=40960),
            "Domaines hérités",
        )
    print(
        "12 cellules Q35, 12 époques, compteurs bloc, refus, reçus EFI et héritage vérifiés."
    )


if __name__ == "__main__":
    main()
