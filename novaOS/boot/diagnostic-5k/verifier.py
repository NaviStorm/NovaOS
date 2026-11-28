"""Relecture en mémoire des preuves publiées. Aucune exécution d'un invité."""

import argparse
import os
from tempfile import TemporaryDirectory
from hashlib import sha256
import json
from pathlib import Path
import re
import struct
import sys
import tarfile

from archiver import DUO, EDU
from verdict_apic import juger

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5i"))
from domaines import legacy


def exiger(ok, nom):
    if not ok:
        raise ValueError(nom)


def verifier_edu(nom, lire):
    def obj(name):
        return json.loads(lire(name))

    row = obj("resultat.json")
    epoch = bytes.fromhex(row["epoque"])
    exiger(lire("epoch.bin") == b"NOVA5J-BOOT-EPOC" + epoch and any(epoch), "Époque")
    signature, lieu, _, code, sb, _, vivants, nonce = struct.unpack(
        "<16s6Q16s", lire("recu-efi.bin")
    )
    exiger(
        signature == b"NOVA5J-RAM-FINAL"
        and lieu
        and code == vivants == 0
        and sb == int(not nom.startswith("inactif"))
        and nonce == epoch,
        "Reçu EFI",
    )
    qmp = lire("compteurs-qmp.jsonl").splitlines()
    exiger(len(qmp) == 2, "Deux relevés QMP")
    for line in qmp:
        (disk,) = (s["stats"] for s in json.loads(line) if s.get("device") == "interne")
        for k in ("rd_bytes", "wr_bytes", "flush_operations"):
            exiger(k in disk, "Compteur manquant")
        exiger(
            not any(
                v for k, v in disk.items() if k.endswith(("_operations", "_bytes"))
            ),
            "E/S internes",
        )
    exiger(
        row["namespace"]
        == sha256(sha256(b"namespace 5K").digest() * 131072).hexdigest(),
        "Namespace",
    )
    root = row["reserve"]
    sid = row["rid"]
    did = row["did"]
    regs = obj("registres.json")
    pre, post = regs["avant"], regs["apres"]
    exiger(
        pre["unites"] == post["unites"] and pre["pci"].keys() == post["pci"].keys(),
        "Inventaire",
    )
    (unit,) = pre["unites"].values()
    exiger(unit["rtaddr"] == root and unit["gsts"] == 0xC4000000, "Racine active")
    for key, config in pre["pci"].items():
        if key == f"0000:00:{sid // 8:02x}.{sid % 8}":
            exiger(
                post["pci"][key] == config | {"cmd": config["cmd"] | 6},
                "BME de la sonde",
            )
        else:
            exiger(post["pci"][key] == config, "PCI voisin")
    raw = lire("avant.bin")
    exiger(len(raw) == 14 * 4096, "Arbres")
    tables = {root + i: raw[i : i + 4096] for i in range(0, len(raw), 4096)}
    decoded = legacy(unit, tables, {root + 4096 * i for i in range(14, 18)})
    exiger(
        decoded == {int(s): d for s, d in row["domaines"].items()}
        and {d["did"] for d in decoded.values()} == {1, 2, 3}
        and did == 3,
        "Domaines",
    )
    pages = [root + 16 * 4096, root + 17 * 4096]
    exiger(
        decoded[sid]
        == dict(
            did=did,
            mappings=[
                dict(iova=4096, cadre=pages[0], droits=1),
                dict(iova=8192, cadre=pages[1], droits=2),
            ],
        ),
        "Permissions du bail",
    )
    for i in (1, 2):
        (voisin,) = (d for d in decoded.values() if d["did"] == i)
        exiger(
            voisin["mappings"]
            == [dict(iova=4096, cadre=root + (13 + i) * 4096, droits=1)],
            "Droits voisins",
        )
    attendu = bytearray(raw)
    offset = 4096 + sid * 16
    attendu[offset : offset + 16] = bytes(16)
    attendu[13 * 4096 + 8 : 13 * 4096 + 24] = bytes(16)
    exiger(lire("apres.bin") == attendu, "Retrait ciblé")
    cookie = int.from_bytes(epoch[8:12], "little") or 1
    motif = int.from_bytes(epoch[:8], "little") or 1
    faute = nom == "iqe"
    entree = 0 if faute else (did << 16) | 0xE2
    queue = struct.pack(
        "<6Q",
        sid << 32 | did << 16 | 0x31,
        0,
        entree,
        0,
        cookie << 32 | 0x25,
        root + 19 * 4096,
    )
    exiger(
        lire("queue.bin") == queue.ljust(4096, b"\0")
        and lire("wait.bin") == struct.pack("<I", row["wait"]).ljust(4096, b"\0"),
        "QI",
    )
    exiger(
        row["titulaire"] == "edu-labo"
        and row["incarnation"] == row["generation"] == 1
        and row["cookie"] == cookie
        and row["en_vol"] == 1
        and row["occupe_final"] == 0
        and row["motif"] == row["echo"] == motif,
        "Achèvement et bail",
    )
    trace = lire("trace.log")
    exiger(
        trace.count(b"vtd_reg_dmar_root addr") == 1
        and trace.count(b"vtd_dmar_enable enable 1") == 1
        and trace.count(b"vtd_inv_desc_cc_device") == 1,
        "Portée de l'invalidation",
    )
    if faute:
        exiger(
            row["iqh"] == 16
            and row["iqt"] == 48
            and row["wait"] == 0
            and row["fsts_qi"] & 16
            and row["pages"] == []
            and row["retenues"] == pages
            and row["bail"] == "quarantaine"
            and b"invalid inv desc" in trace
            and b"vtd_inv_desc_wait_sw" not in trace,
            "Refus sur IQE",
        )
    else:
        exiger(
            row["iqh"] == row["iqt"] == 48
            and row["wait"] == cookie
            and row["fsts_qi"] == 0
            and row["pages"] == pages
            and row["retenues"] == []
            and row["bail"] == "rendu",
            "Attente matérielle",
        )
        exiger(
            f"vtd_inv_desc_iotlb_domain iotlb invalidate whole domain {did:#x}".encode()
            in trace
            and f"vtd_inv_desc_wait_sw wait invalidate status write addr {root + 19 * 4096:#x} data {cookie:#x}".encode()
            in trace
            and f"vtd_dmar_fault sid {sid:#x} fault 2 addr 0x2000 write 1".encode()
            in trace
            and row["fsts"] & 2
            and row["post_dma"] == row["nouveau"] == motif ^ 0xFFFFFFFFFFFFFFFF,
            "Réaffectation",
        )
    if nom.endswith("-apic"):
        proof = obj("interruptions.json")
        verdict = juger(trace, lire("intervalle.log"), proof)
        exiger(
            verdict == obj("verdict-apic.json")
            and not verdict["intervalle_admis"]
            and len(verdict["ecritures_refusees"]) == 6
            and all(
                re.fullmatch(
                    r"memory_region_ops_write cpu 0 mr 0x[0-9a-f]+ addr 0xb[23] value 0x0 size 1 name 'apm-io'",
                    w["ligne"],
                )
                for w in verdict["ecritures_refusees"]
            ),
            "Refus APM",
        )
    return epoch


def verifier_duo(nom, lire, lecteur):
    d = json.loads(lire("campagne.json"))
    disque = lire("namespace.raw")
    s = d["stockage"]
    trace = lire("qemu.trace")
    serie = lire("serie.log")
    exiger(
        d["succes"]
        and d["cas"] == nom
        and len(disque) == 4 << 20
        and sha256(disque).hexdigest() == d["p9_final"],
        "Namespace duo",
    )
    if lecteur:
        from nvme_phase2_oracle import inspect

        exiger(
            sha256(lecteur.read_bytes()).hexdigest() == d["lecteur"],
            "Identité du lecteur NovaFS",
        )
        with TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp:
            image = Path(tmp) / "namespace.raw"
            image.write_bytes(disque)
            retrouve = inspect(lecteur, image, 512)
        exiger(
            json.loads(json.dumps(retrouve)) == s["p9"],
            "Ancres, garde ou contenu NovaFS",
        )
    guard = bytes((i * 11 + (i >> 8) + 67) & 255 for i in range(4096))
    fichier = bytes((i * 73 & 255) ^ (i >> 5 & 255) for i in range(4231))
    exiger(
        disque[-4096:] == guard
        and sha256(disque[:-4096]).hexdigest() == s["p9"]["sha256_p9"]
        and s["p9"]["sha256_fichier"] == sha256(fichier).hexdigest()
        and s["p9"]["etat"] == "nouveau",
        "Transaction NovaFS",
    )
    exiger(
        trace.count(b"pci_nvme_io_cmd ") == 32 and b"NVME" in serie, "Commandes NVMe"
    )
    if nom in ("duo-retrait", "duo-differe"):
        exiger(
            s["cadres_nic_retenus"] == 0
            and s["E_S_apres_WAIT_NIC"] > 0
            and b"liberes=26 doublon=refuse NVMe_actif=true" in serie,
            "Retrait avec NVMe actif",
        )
    elif nom == "duo-sans-reset":
        exiger(
            s["cadres_nic_retenus"] == 26 and s["E_S_apres_BME_NIC"] > 0,
            "Quarantaine NIC",
        )
    else:
        exiger(
            (s["controleur"], s["nic"], s["nvme"]) == (8, 26, 6)
            and b"deux_retraits_refuses" in serie
            and b"NVME quarantaine cadres=6" in serie,
            "Quarantaine commune",
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--lecteur-p9",
        type=Path,
        help="Lecteur historique natif pour remonter également les cinq transactions",
    )
    args = parser.parse_args()
    sys.path.extend([str(NOVA / "tests"), str(NOVA / "scripts")])
    base = NOVA / "docs/validation/diagnostic-uefi-5k"
    for line in (base / "SHA256SUMS").read_text().splitlines():
        sha, nom = line.split()
        exiger(sha256((base / nom).read_bytes()).hexdigest() == sha, nom)
    with tarfile.open(base / "observations.tar.gz") as archive:
        epochs = set()
        for nom in EDU + DUO:

            def lire(piece):
                return archive.extractfile(nom + "/" + piece).read()

            if nom in EDU:
                epochs.add(verifier_edu(nom, lire))
            else:
                verifier_duo(nom, lire, args.lecteur_p9)
        exiger(len(epochs) == len(EDU), "Époque dupliquée")
    if args.lecteur_p9:
        print("5 namespaces remontés par le lecteur NovaFS P9 indépendant.")
    print("5 démarrages EDU, 5 démarrages NIC/NVMe et 2 intervalles EFI vérifiés.")


if __name__ == "__main__":
    main()
