"""Contre-lecture sans VM des témoins utilisés pour la décision 5S."""

import argparse
import json
import platform
import struct
import sys
import tempfile
import zipfile
import zlib
from hashlib import sha256
from pathlib import Path

NOVA = Path(__file__).resolve().parents[2]
PREUVES = NOVA / "docs/validation"
sys.path.insert(0, str(NOVA / "boot/diagnostic-5r"))
import qualification as r


def exiger(ok, cause):
    if not ok:
        raise ValueError(cause)


def ppm_depuis_png(blob):
    # Encodage RGB sans filtrage de la routine publique 5O. Reconstituer le PPM
    # original pour le comparer à l'empreinte relevée par le banc en cours d'essai.
    exiger(blob[:8] == b"\x89PNG\r\n\x1a\n", "Signature PNG")
    offset = 8
    morceaux = []
    for nom in (b"IHDR", b"IDAT", b"IEND"):
        n = struct.unpack_from(">I", blob, offset)[0]
        data = blob[offset + 8 : offset + 8 + n]
        exiger(blob[offset + 4 : offset + 8] == nom and len(data) == n, "Chunk PNG")
        exiger(
            zlib.crc32(nom + data) == struct.unpack_from(">I", blob, offset + 8 + n)[0],
            "CRC PNG",
        )
        morceaux.append(data)
        offset += 12 + n
    exiger(offset == len(blob) and not morceaux[2], "Fin PNG")
    w, h, bits, rgb, compression, filtre, entrelacs = struct.unpack(
        ">IIBBBBB", morceaux[0]
    )
    exiger(
        (w, h, bits, rgb, compression, filtre, entrelacs) == (1280, 800, 8, 2, 0, 0, 0),
        "Géométrie observée",
    )
    contenu = zlib.decompress(morceaux[1])
    pas = 1 + 3 * w
    exiger(len(contenu) == pas * h and not any(contenu[::pas]), "Format de scanline")
    return f"P6\n{w} {h}\n255\n".encode() + b"".join(
        contenu[i + 1 : i + pas] for i in range(0, len(contenu), pas)
    )


def verifier(archive, pe_path):
    pe = pe_path.read_bytes()
    exiger(r.empreinte(pe) == r.PE, "PE 5R")
    v = PREUVES / "diagnostic-uefi-5r"
    store = json.loads((v / "variables.json").read_bytes())
    with (
        zipfile.ZipFile(v / "observations.zip") as z,
        tempfile.TemporaryDirectory() as tmp,
    ):
        for mode in ("actif", "inactif"):
            for cas in ("nominal", "surface"):

                def lire(n, prefix=f"{mode}-{cas}/"):
                    return z.read(prefix + n)

                r.cellule(mode, cas, lire, pe, Path(tmp), None, store)
    print("5R : 4 cellules historiques contre-lues (nominal, surface, deux modes).")
    f = json.loads((PREUVES / "diagnostic-uefi-5f/bilan.json").read_bytes())["cas"]
    for topo in ("direct", "pont"):
        admis = f[topo]["ordre-usb"]
        refus = f[topo]["prochain-usb-refuse"]
        exiger(
            admis["entree"]["rd_operations"] == 6
            and admis["rapport"]["statut"] == 0
            and refus["entree"]["rd_operations"] == 11
            and refus["refus_usb"]
            and refus["rapport"]["statut"] == 9,
            "Repli historique 5F",
        )
    hors_db = json.loads((PREUVES / "diagnostic-uefi-5n/modes.json").read_bytes())[
        "actif"
    ]
    exiger(
        not hors_db["lancement"]
        and hors_db["compteurs_terminal"]["interne"]["rd_operations"] == 40,
        "Repli 5N",
    )
    print(
        "5F : 6 lectures avant admission et 11 avec repli interne sur deux topologies. 5N : 40 lectures sans lancement."
    )
    with zipfile.ZipFile(archive) as z:
        noms = {
            f"{m}/{n}"
            for m in ("actif", "inactif")
            for n in (
                "observation.json",
                "entree.png",
                "hlt.png",
                "commandes-intervalle.log",
            )
        }
        exiger(
            len(z.namelist()) == len(noms)
            and set(z.namelist()) == noms
            and z.testzip() is None,
            "Pièces 5S",
        )
        for mode in ("actif", "inactif"):
            pre = mode + "/"
            b = json.loads(z.read(pre + "observation.json"))
            exiger(
                b["mode_banc"] == mode
                and b["pe"] == r.PE
                and b["image"] == r.IMAGE
                and b["firmware"]
                == "32807682a9e5c0e2d192ecb6077941d4be8bcfa6b9cd6e60830c3a9f48c0555a"
                and b["magasin_initial"]
                == (store["copie"] if mode == "actif" else r.q.STORES["inactif"]),
                "Identité du témoin",
            )
            meta = r.q.pe_info(pe)
            exiger(
                b["entree"] - meta["entree"] == b["hlt"] - meta["hlt"] > 0
                and b["sections_verifiees"] == 5
                and b["verdict"] == {"statut": 16, "confiance": 2, "arena": 2880, "vivants": 0}
                and b["dma_precoce_usb"] == "indeterminee"
                and b["lecture_seule"]
                and b["supports_usb"] == 1
                and "QEMU USB HARDDRIVE" in b["origine"]
                and b["vm_arretee"],
                "Trajet du refus",
            )
            exiger(
                b["compteurs_entree"] == b["compteurs_hlt"]
                and b["interne_intact"]
                and b["trace_commandes_sata_octets"] == 0
                and not z.read(pre + "commandes-intervalle.log"),
                "E/S diagnostic",
            )
            for point in ("compteurs_reset", "compteurs_entree", "compteurs_hlt"):
                interne = b[point]["interne"]
                exiger(
                    set(interne)
                    == {
                        "flush_operations",
                        "rd_bytes",
                        "rd_operations",
                        "unmap_operations",
                        "wr_bytes",
                        "wr_operations",
                    }
                    and not any(interne.values()),
                    "Compteurs du disque inclus",
                )
            usb = b["compteurs_entree"]["usb5n"]
            exiger(
                not any(b["compteurs_reset"]["usb5n"].values())
                and usb["rd_operations"] == 35
                and usb["rd_bytes"] == 538624
                and not usb["wr_bytes"],
                "Lecture USB effective",
            )
            a = z.read(pre + "entree.png")
            exiger(
                a == z.read(pre + "hlt.png")
                and sha256(ppm_depuis_png(a)).hexdigest()
                == b["sha256_ppm_entree"]
                == b["sha256_ppm_hlt"],
                "Écran",
            )
            print(
                f"5S {mode} : statut 16, confiance indéterminée, cinq sections, écran identique, zéro E/S interne."
            )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("pe", type=Path)
    p.add_argument(
        "--archive", type=Path, default=PREUVES / "diagnostic-uefi-5s/temoin.zip"
    )
    args = p.parse_args()
    print(platform.system(), platform.machine(), platform.python_version())
    verifier(args.archive, args.pe)
