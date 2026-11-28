"""Archivage sur liste fermée et vérification hors VM des phases du chargement USB."""

import argparse
import json
from pathlib import Path
import re
import struct
import sys
import tempfile
import zipfile

from politique import FIRMWARE, PAGE, Registre, canon, empreinte, exiger, source_usb

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5p"))
from preuves_usb import (
    IMAGE,
    anonymiser,
    activite,
    chargeur,
    images,
    juger,
    legacy,
    sections_chargees,
    signe,
)
from origine import chemin

from archiver import png
from image import pe_info
from demarrage import STORES

MODES = ("actif", "inactif")
CAS = (
    "nominal",
    "vol",
    "invalide",
    "clone",
    "reconnexion",
    "signature",
    "contradiction",
    "perime",
)
SUCCES = ("nominal", "vol")
REJET = ("signature", "contradiction", "perime")
TRACE_TRAD = re.compile(
    rb"(?m)^vtd_iotlb_page_update IOTLB page update sid 0x([0-9a-f]+) iova 0x([0-9a-f]+) slpte 0x([0-9a-f]+) domain 0x([0-9a-f]+)$"
)
BASE = NOVA / "docs/validation/diagnostic-uefi-5q"


def pieces(cas):
    noms = ["bilan.json", "cpu.log", "trace.log", "initial.bin"]
    if cas not in ("clone", "invalide"):
        noms.append("avant-load.bin")
    if cas in SUCCES + REJET:
        noms.extend(
            ("contrat.json", "transfert.bin", "origine.bin", "surface.bin", "recu.bin")
        )
    if cas in SUCCES:
        noms.extend(
            (
                "entree.bin",
                "apres.bin",
                "avant.ppm",
                "apres.ppm",
                "interruptions.json",
                "intervalle.log",
                "verdict-apic.json",
                "sortie.bin",
            )
        )
    return noms


def archive(folder, out):
    with zipfile.ZipFile(out, "x") as z:
        for mode in MODES:
            for cas in CAS:
                sub = mode + "-" + cas
                p = folder / sub
                trace = anonymiser((p / "trace.log").read_bytes())
                # Les objets MemoryRegion sont des pointeurs hôtes. La valeur de ce
                # champ n'intervient pas dans l'attribution par adresse MMIO invitée.
                trace = re.sub(
                    rb"(?<= mr )0x[0-9a-f]+",
                    lambda m: b"0x" + b"0" * (len(m[0]) - 2),
                    trace,
                )
                for name in pieces(cas):
                    if name == "trace.log":
                        data = trace
                    elif name == "intervalle.log":
                        obs = json.loads((p / "interruptions.json").read_bytes())
                        data = trace[obs["debut"] : obs["fin"]]
                    else:
                        data = (p / name).read_bytes()
                    item = zipfile.ZipInfo(sub + "/" + name, (2000, 1, 1, 0, 0, 0))
                    item.external_attr = 0o100644 << 16
                    z.writestr(item, data, zipfile.ZIP_DEFLATED, compresslevel=9)


def verif_tables(blob, b, ledger):
    indices = (*range(14), 20, 21, 22, 23)
    exiger(len(blob) == len(indices) * PAGE, "Nombre de pages de table")
    base = b["reserve"]
    pages = {
        base + i * PAGE: blob[n * PAGE : (n + 1) * PAGE] for n, i in enumerate(indices)
    }
    autorise = {base + i * PAGE for i in range(14, 18)}
    feuilles = ledger.feuilles()
    domaines = legacy(b["unite"], pages, autorise | {f["cadre"] for f in feuilles})
    ids = b["rids"]
    fixes = {}
    for nom, did, slots in (
        ("nic", 2, [15]),
        ("stock", 1, [14]),
        ("sonde", 3, [16, 17]),
    ):
        fixes[ids[nom]] = dict(
            did=did,
            mappings=[
                dict(iova=(i + 1) * PAGE, cadre=base + slot * PAGE, droits=i + 1)
                for i, slot in enumerate(slots)
            ],
        )
    exiger(
        domaines == fixes | {ids["xhci"]: dict(did=4, mappings=feuilles)},
        "Domaine ou feuille inattendus",
    )


def traductions(trace, b, ledger):
    feuilles = {f["iova"]: f for f in ledger.feuilles()}
    publications = b["publications"]
    exiger(
        len(publications) == len(ledger.loans) and publications == sorted(publications),
        "Chronologie des prêts",
    )
    seuil = {
        l["iova"] + i * PAGE: seq
        for l, seq in zip(ledger.loans, publications)
        for i in range(l["pages"])
    }
    vus = set()
    for m in TRACE_TRAD.finditer(trace):
        rid, iova, pte, did = (int(v, 16) for v in m.groups())
        page = iova & ~(PAGE - 1)
        f = feuilles.get(page)
        exiger(
            rid == b["rids"]["xhci"]
            and did == 4
            and f
            and pte == f["cadre"] | f["droits"]
            and seuil[page] <= m.start(),
            "Traduction sans prêt antérieur",
        )
        vus.add(page)
    exiger(
        vus == set(feuilles) and len(vus) > 0,
        "Couverture des feuilles effectivement utilisées",
    )
    return len(vus)


def cellule(mode, cas, lire, key, tmp):
    b = json.loads(lire("bilan.json"))
    cpu = lire("cpu.log").decode().splitlines()
    tr = lire("trace.log")
    epoch = b["epoque"]
    pe = chargeur()
    rid = b["rids"]["xhci"]
    ledger = Registre(b["reserve"], rid, epoch)
    exiger(
        (b["mode"], b["cas"]) == (mode, cas)
        and b["image"] == b["image_apres"] == IMAGE
        and b["pe"] == empreinte(pe)
        and b["firmware"] == FIRMWARE
        and b["magasin"]
        == (
            "bf41a49e08223f8e0a0192371d8fb79602faf56e4c0a58a5ad79624a54f2ed8f"
            if mode == "actif"
            else STORES["inactif"]
        ),
        "Artefacts",
    )
    exiger(
        b["reserve"] == 1 << 32
        and b["stockage_apres"] == empreinte(bytes(range(256)) * 16384),
        "Disque interne synthétique",
    )
    exiger(
        all(not any(b[k]["interne"].values()) for k in ("avant", "apres"))
        and all(
            not stat["wr_bytes"] and not stat["wr_operations"]
            for stat in b["apres"].values()
        ),
        "E/S disque",
    )
    exiger(
        b"vtd_dmar_enable enable 1" in tr[: b.get("debut_vcpu", len(tr))]
        and not re.search(
            rb"(?m)^(?:vtd_dmar_fault |pci_nvme_(?:io_cmd|admin_cmd|mmio_doorbell))", tr
        ),
        "Événement matériel inattendu",
    )
    verif_tables(lire("initial.bin"), b, ledger)
    markers = []

    def cpu_attendu():
        exiger(
            cpu
            == ["initialise"]
            + [f"marqueur cpu=0 cellule={k} valeur={v}" for k, v in markers],
            "Écritures du vCPU",
        )

    if cas == "clone":
        exiger(
            len(b["origine_qmp"]) == 2
            and b["clone_sha256"] == IMAGE
            and b["decision"] == "refus-clone"
            and not b["publications"],
            "Admission d'un clone",
        )
        cpu_attendu()
        return epoch
    exiger(
        not re.search(rb"(?m)^vtd_reg_write ", tr[b["debut_vcpu"] :]),
        "Écriture VT-d du firmware",
    )
    if cas == "invalide":
        exiger(
            b["refus_firmware"] == 2
            and b["decision"] == "refus-mapping"
            and not b["publications"],
            "Refus d'allocation",
        )
        cpu_attendu()
        return epoch
    for l in b["prets"]:
        ledger.confirmer(l)
    maps = lire("avant-load.bin")
    verif_tables(maps, b, ledger)
    before = b["avant_load"]
    usb = before["compteurs"]["usb5n"]
    exiger(
        usb["rd_bytes"] >= before["octets"] == 33 * (1 << 20)
        and usb["rd_operations"] > 0
        and (cas == "reconnexion" or b["apres"].get("usb5n") == usb),
        "Lecture effective de l'ESP",
    )
    traces = tr[: before["position"]]
    traductions(traces, b, ledger)
    activ = activite(traces.decode().splitlines())
    exiger(
        not activ["vols"]
        and not activ["anomalies"]
        and activ["luns"] == [0]
        and activ["adresses"] == [[1, "1"]],
        "Transactions xHCI",
    )
    markers.append((27, 1))
    if cas == "reconnexion":
        exiger(
            "usb5n" not in b["apres"]
            and not any(b["apres"]["clone5n"].values())
            and not b["deconnecte"]
            and len(b["reconnecte"]) == 1
            and "ID: autre" in b["reconnecte"][0]
            and b["clone_sha256"] == IMAGE
            and b["decision"] == "quarantaine-reconnexion",
            "Perte d'incarnation",
        )
        cpu_attendu()
        return epoch
    markers.append((27, 2))
    co = json.loads(lire("contrat.json"))
    signed = lire("transfert.bin")
    raw = lire("origine.bin")
    nodes = chemin(lambda a, n: raw[a : a + n], 0)
    part = ["partition", 1, 2048, 67584, "fc9edb7aa8daa8529eccbbc3298d92f5", 2, 2]
    port = source_usb(nodes, rid, part)
    exiger(
        0 < len(raw) <= 128
        and port == co["port"]
        and co["source"] == nodes
        and co["incarnation"] == 1
        and co["route"] == []
        and co["lun"] == 0,
        "Origine du support synthétique",
    )
    exiger(
        co["epoque"] == epoch
        and co["secure_boot"] == mode
        and co["tables"] == empreinte(maps)
        and co["ressources"] == ledger.loans
        and co["image"] == IMAGE
        and co["pe"] == empreinte(pe)
        and co["esp"] == before["sha256"],
        "Propriétés attestées",
    )
    exiger(
        co["domaines"] == b["rids"]
        and co["rid"] == rid
        and co["transactions"]["soumises"] == co["transactions"]["achevees"] > 0,
        "Propriété ou clôture",
    )
    exiger(
        signed[288:-64] == canon(co)
        and signed[:16] == b"NOVA5Q-USB-V1!!!"
        and signed[32:160]
        == bytes.fromhex(co["esp"] + co["pe"] + empreinte(raw) + empreinte(canon(co))),
        "Intégrité du contrat",
    )
    exiger(
        signe(signed, key, tmp) == (cas != "signature")
        and (signed[16:32].hex() == epoch) == (cas != "perime"),
        "Authenticité ou fraîcheur",
    )
    facts = struct.unpack_from("<16Q", signed, 160)
    base = b["avant_start"]["base"]

    tail = pe_info(pe)["taille"]
    exiger(
        facts[0] == (rid ^ 8 if cas == "contradiction" else rid)
        and facts[1:14]
        == (
            1,
            int(mode == "actif"),
            port,
            co["transactions"]["soumises"],
            co["transactions"]["achevees"],
            base,
            tail,
            *co["surface"],
        )
        and facts[15] == len(pe)
        and facts[14] + len(pe) <= before["octets"],
        "Comparaison des champs du BDS",
    )
    exiger(
        co["sections"] == {n: list(v) for n, v in sections_chargees(pe, base).items()},
        "Sections du PE",
    )
    if cas in REJET:
        exiger(b["decision"] == "refus-" + cas, "Démarrage après refus")
        cpu_attendu()
        return epoch
    markers.extend(((29, 1), (27, 3)))
    cpu_attendu()
    exiger(
        lire("entree.bin") == maps == lire("apres.bin")
        and base + tail < b["reserve"]
        and 0 < b["pile_entree"] < b["reserve"]
        and co["surface"][0] + co["surface"][1] < b["reserve"],
        "Pages protégées",
    )
    windows = json.loads(lire("interruptions.json"))
    interval = lire("intervalle.log")
    apic = juger(tr, interval, windows)
    exiger(
        apic == json.loads(lire("verdict-apic.json"))
        and apic["intervalle_admis"]
        and not re.search(
            rb"(?m)^(?:apm_io_write|usb_msd_cmd_submit|virtio_notify|vtd_reg_write|pci_nvme_)",
            interval,
        ),
        "Intervalle du diagnostic",
    )
    exiger(
        signe(lire("surface.bin"), key, tmp)
        and signe(lire("recu.bin"), key, tmp)
        and lire("surface.bin")[16:32].hex() == lire("recu.bin")[32:48].hex() == epoch
        and lire("recu.bin")[48:80].hex() == empreinte(pe)
        and lire("recu.bin")[80:112] == key,
        "Époque du diagnostic",
    )
    avant = lire("avant.ppm")
    apres = lire("apres.ppm")
    images(
        "nominal",
        dict(
            surface=co["surface"],
            rendu=True,
            octets_fenetre=544 * 120 * 4,
            avant_ppm=empreinte(avant),
            apres_ppm=empreinte(apres),
        ),
        avant,
        apres,
    )

    exiger(
        png(apres)
        == (
            NOVA / f"docs/validation/diagnostic-uefi-5o/verdict-{mode}.png"
        ).read_bytes(),
        "Pixels de refus différents de 5O",
    )
    marque, addr, status, sb, arena, live, ep, digest = struct.unpack(
        "<16s5Q16s32s", lire("sortie.bin")
    )
    exiger(
        marque == b"NOVA5M-RAM-FINAL"
        and b["decision"] == "refus-affiche"
        and b["reads_before"] == b["apres"]
        and ep.hex() == epoch
        and digest.hex() == co["pe"]
        and status == 40
        and sb == int(mode == "actif")
        and live == 0
        and base <= addr < base + tail
        and arena > 0,
        "Sortie terminale",
    )
    if cas == "vol":
        v = b["en_vol"]
        state = activite(tr[: v["position"]].decode().splitlines())
        exiger(
            v["soumis"] > v["clos"]
            and not state["anomalies"]
            and [list(row) for row in state["vols"]] == v["requetes"]
            and any(e > 1 and busy for _, e, busy in state["vols"]),
            "Requête asynchrone absente",
        )
    return epoch


def verifier(path):
    key = (NOVA / "boot/diagnostic-5m/autorite-laboratoire.pub").read_bytes()
    expected = {f"{m}-{c}/{p}" for m in MODES for c in CAS for p in pieces(c)}
    epochs = set()
    with zipfile.ZipFile(path) as z, tempfile.TemporaryDirectory() as tmp:
        exiger(
            len(z.namelist()) == len(expected)
            and set(z.namelist()) == expected
            and not z.testzip(),
            "Archive incomplète",
        )
        for m in MODES:
            for c in CAS:
                ep = cellule(m, c, lambda n: z.read(f"{m}-{c}/{n}"), key, Path(tmp))
                exiger(ep not in epochs, "Époque réutilisée")
                epochs.add(ep)
    print(
        f"{len(epochs)} époques distinctes, 4 chargements et affichages, 12 refus préalables, {len(expected)} pièces vérifiées."
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    s = p.add_subparsers(dest="cmd", required=True)
    a = s.add_parser("archiver")
    a.add_argument("dossier", type=Path)
    a.add_argument("archive", type=Path)
    v = s.add_parser("verifier")
    v.add_argument("archive", type=Path, nargs="?", default=BASE / "observations.zip")
    args = p.parse_args()
    if args.cmd == "archiver":
        archive(args.dossier, args.archive)
    else:
        verifier(args.archive)
