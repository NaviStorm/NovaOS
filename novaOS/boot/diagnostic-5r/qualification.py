"""Archivage fermé et contre-lecture du diagnostic à partir des traces et de la RAM."""

import argparse
import json
import re
import struct
import sys
import tempfile
import zipfile
from pathlib import Path

from scenarios import CAS, CHAMPS

DOSSIER = Path(__file__).resolve().parent
NOVA = DOSSIER.parents[1]
sys.path.insert(0, str(DOSSIER.parent / "diagnostic-5q"))
import preuves as q
from politique import FIRMWARE, Registre, canon, empreinte, exiger

BASE = NOVA / "docs/validation/diagnostic-uefi-5r"
PE = "7f80bf3c42c89162d67fb229b9080e9706c0f7f6a3574978051c1c0c555477a5"
IMAGE = "9440a1030b530174d97e85e4e55cb6566c514dfd3a143a68fff77ae8b5f1b404"
CLE = (DOSSIER.parent / "diagnostic-5m/autorite-laboratoire.pub").read_bytes()
REFUS = {
    6: "MANIFESTE INVALIDE",
    18: "PAQUET ELF INVALIDE",
    50: "CONTRAT ABSENT OU FORMAT",
    51: "SIGNATURE USB INVALIDE",
    52: "EPOQUE OU INCARNATION",
    53: "IDENTITE PE INVALIDE",
    54: "ORIGINE USB INCOHERENTE",
    55: "MAPPINGS DMA INCOHERENTS",
    56: "TRANSACTION EN VOL",
    57: "SUBSTITUTION DU CONTRAT",
}
AVANT = {
    "clone": "refus-clone",
    "reconnexion": "quarantaine-reconnexion",
    "substitue": "refus-pe-substitue",
    "hors-db": "refus-ovmf",
    "quarantaine-vol": "quarantaine-en-vol",
}


def cellules():
    return [
        (m, c)
        for m in ("inactif", "actif")
        for c in CAS
        if c != "hors-db" or m == "actif"
    ]


def pieces(m, c):
    noms = ["bilan.json", "cpu.log", "initial.bin", "trace.log"]
    if m == "actif":
        noms.append("variables.json")
    if c == "quarantaine-vol":
        return noms + ["quarantaine.bin"]
    if c == "clone":
        return noms
    noms.append("avant-load.bin")
    if c in AVANT:
        return noms
    return noms + [
        "apres.bin",
        "apres.ppm",
        "avant.ppm",
        "contrat.json",
        "details.bin",
        "entree.bin",
        "entree-remise.bin",
        "arret-remise.bin",
        "interruptions.json",
        "intervalle.log",
        "journal.txt",
        "livraison.bin",
        "origine.bin",
        "rapport.json",
        "recu.bin",
        "sortie.bin",
        "surface.bin",
        "transfert.bin",
        "verdict-apic.json",
    ]


def archiver(inactif, actif, sortie):
    with zipfile.ZipFile(sortie, "x") as z:
        for m, c in cellules():
            sub = f"{m}-{c}"
            p = (actif if m == "actif" else inactif) / sub
            trace = q.anonymiser((p / "trace.log").read_bytes())
            trace = re.sub(
                rb"(?<= mr )0x[0-9a-f]+",
                lambda v: b"0x" + b"0" * (len(v[0]) - 2),
                trace,
            )
            for name in pieces(m, c):
                if name == "trace.log":
                    data = trace
                elif name == "intervalle.log":
                    obs = json.loads((p / "interruptions.json").read_bytes())
                    data = trace[obs["debut"] : obs["fin"]]
                else:
                    data = (p / name).read_bytes()
                info = zipfile.ZipInfo(sub + "/" + name, (2000, 1, 1, 0, 0, 0))
                info.external_attr = 0o100644 << 16
                z.writestr(info, data, zipfile.ZIP_DEFLATED, compresslevel=9)


def decoder_ecran(avant, apres, surface, r):
    code = r["statut"]
    q.images(
        "refus" if code == 44 else "nominal",
        {
            "surface": surface,
            "rendu": code != 44,
            "octets_fenetre": 544 * 120 * 4,
            "avant_ppm": empreinte(avant),
            "apres_ppm": empreinte(apres),
        },
        avant,
        apres,
    )
    if code == 44:
        return
    lignes = [
        "NOVAOS  DIAGNOSTIC USB VALIDE" if not code else f"NOVAOS  REFUS {code}",
        REFUS[code] if code else "USB AUTHENTIFIE",
        "SECURE BOOT " + r["secure_boot"].upper(),
        "DMA PROTEGEE  BANC Q35" if r["usb"] else "DMA USB NON VALIDEE",
        "PCI {pci}   IOAPIC {ioapic}   HPET {hpet}   DRHD {drhd}".format(**r),
        "PILOTE COMPATIBLE  IOAPIC" if r["compatibles"] else "AUCUNE SELECTION PUBLIEE",
        "HPET A ET B  CONFLIT" if r["conflits"] else "HPET  AUCUN PLAN PUBLIE",
        "ACTIVATION MATERIELLE  AUCUNE",
    ]
    # Décoder la grille de glyphes depuis la capture QEMU. Contrôler également les pixels
    # de bord, le doublement des points et l'arrière-plan de tout le rectangle.
    alphabet = {" ": [0] * 7}
    for fichier in [DOSSIER.parent / "diagnostic-5o/ecran.rs", DOSSIER / "ecran.rs"]:
        for char, rows in re.findall(
            r"b'([A-Z0-9])' => \[([0-9, ]+)\]", fichier.read_text()
        ):
            alphabet[char] = list(map(int, rows.split(",")))
    pixels = apres.split(b"\n", 3)[3]
    width = surface[2]
    for y in range(120):
        row = bytearray(b"\x16" * (544 * 3))
        line, dy = divmod(y - 3, 14)
        if 0 <= line < len(lignes):
            for i, ch in enumerate(lignes[line]):
                bits = alphabet[ch][dy // 2]
                for x in range(5):
                    if bits & (16 >> x):
                        pos = 3 * (8 + 12 * i + 2 * x)
                        row[pos : pos + 6] = b"\xff" * 6
        exiger(
            pixels[3 * y * width : 3 * y * width + len(row)] == row,
            "Texte du verdict ou pixels inattendus",
        )


def champs(blob):
    grant = blob[:232]
    reste = blob[232:]
    champs = []
    for maxi in (6144, 128, 1024, 64, 512):
        exiger(len(reste) >= 4, "Champ tronqué")
        (n,) = struct.unpack_from("<I", reste)
        exiger(n <= maxi and 4 + n <= len(reste), "Limite du champ")
        champs.append(reste[4 : 4 + n])
        reste = reste[4 + n :]
    exiger(not reste, "Données supplémentaires")
    return grant, champs


def remise(m, c, b, lire, tmp, precedent):
    signed = lire("transfert.bin")
    old, rec = lire("surface.bin"), lire("recu.bin")
    grant, (received, path, manifest, sig, elf) = champs(lire("livraison.bin"))
    co = json.loads(lire("contrat.json"))
    ep = bytes.fromhex(b["epoque"])
    pe = bytes.fromhex(PE)
    exiger(
        all(q.signe(i, CLE, tmp) for i in (old, rec, grant, signed)),
        "Signature de la remise initiale",
    )
    exiger(
        signed[:16] == b"NOVA5Q-USB-V1!!!"
        and signed[16:32] == ep
        and signed[288:-64] == canon(co)
        and signed[32:160]
        == bytes.fromhex(co["esp"] + PE + empreinte(path) + empreinte(canon(co))),
        "Contrat 5Q initial",
    )
    exiger(
        rec[32:112] == ep + pe + CLE
        and rec[16] == int(m == "actif")
        and old[:16] == b"NOVA5O-DISPLAY1!"
        and grant[:16] == b"NOVA5R-DISPLAY1!"
        and old[16:64] == grant[16:64] == ep + pe
        and grant[96:104] == bytes([int(m == "actif"), 1, 1, 0, 0, 0, 0, 0])
        and old[96:104] == bytes([int(m == "actif"), 1, 1, 40, 0, 0, 0, 0])
        and grant[104:168] == bytes.fromhex(empreinte(signed) + b["image"]),
        "Liaison entre reçus",
    )
    exiger(
        (grant[64:96] != old[64:96]) == (c == "surface")
        and struct.unpack_from("<QQIIII", old, 64) == tuple(co["surface"]),
        "Propriété de la surface",
    )
    nodes = q.chemin(lambda a, n: path[a : a + n], 0)
    # Géométrie GPT de la seule image synthétique identifiée par IMAGE.
    partition = ["partition", 1, 2048, 67584, "fc9edb7aa8daa8529eccbbc3298d92f5", 2, 2]
    exiger(
        path == lire("origine.bin")
        and nodes == co["source"] == b["source"]
        and q.source_usb(nodes, b["rids"]["xhci"], partition) == co["port"]
        and co["incarnation"] == 1
        and co["route"] == []
        and co["lun"] == 0
        and co["domaines"] == b["rids"]
        and co["ressources"] == b["prets"]
        and co["tables"] == empreinte(lire("avant-load.bin"))
        and co["epoque"] == ep.hex()
        and co["pe"] == PE
        and co["esp"] == b["avant_load"]["sha256"]
        and co["image"] == IMAGE
        and co["secure_boot"] == m
        and co["sections"] == b["avant_start"]["sections"],
        "Origine, mappings ou image",
    )
    t = co["transactions"]
    words = struct.unpack_from("<16Q", signed, 160)
    exiger(
        t["soumises"] == t["achevees"] > 0
        and words[:7]
        == (
            co["rid"],
            1,
            int(m == "actif"),
            co["port"],
            t["soumises"],
            t["achevees"],
            b["avant_start"]["base"],
        )
        and words[8:14] == tuple(co["surface"]),
        "Comparaison du préfixe BDS",
    )
    fix = DOSSIER.parent / "diagnostic-5e/vecteurs"
    exiger(
        manifest == (fix / "ioapic.toml").read_bytes()
        and (sig != (fix / "ioapic.sig").read_bytes()) == (c == "manifeste")
        and (
            elf != (NOVA / "libs/catalogue-pilotes/vecteurs/x86_64.pkg01").read_bytes()
        )
        == (c == "elf"),
        "Vecteurs catalogue",
    )
    if c == "absent":
        exiger(not received, "Contrat présent")
    elif c in ("altere", "autorite"):
        exiger(
            received[:-64] == signed[:-64] and not q.signe(received, CLE, tmp),
            "Mauvaise signature acceptée",
        )
    elif c == "reutilise":
        exiger(received == precedent and received[16:32] != ep, "Rejeu inter-époques")
    elif c in (
        "mapping",
        "origine",
        "epoque",
        "identite-pe",
        "en-cours",
        "substitution-contrat",
    ):
        exiger(
            q.signe(received, CLE, tmp) and received != signed,
            "Injection non authentifiée",
        )
        other = json.loads(received[288:-64])
        attendu = {
            "epoque": received[16:32] != ep,
            "identite-pe": received[64:96] != pe,
            "origine": other["source"] != nodes,
            "mapping": other["ressources"] != b["prets"],
            "en-cours": other["transactions"]["soumises"]
            > other["transactions"]["achevees"],
            "substitution-contrat": other["transactions"]["soumises"]
            == other["transactions"]["achevees"]
            == t["achevees"] + 1,
        }
        exiger(attendu[c], "Motif de l'injection")
    else:
        exiger(received == signed, "Altération imprévue")
    return co


def cellule(m, c, lire, pe, tmp, precedent, store):
    b = json.loads(lire("bilan.json"))
    epoch = b["epoque"]
    exiger(
        len(bytes.fromhex(epoch)) == 16 and (b["mode"], b["cas"]) == (m, c),
        "Époque ou scénario",
    )
    expected_pe = q.chargeur() if c == "substitue" else pe
    exiger(
        b["firmware"] == FIRMWARE
        and b["pe"] == empreinte(expected_pe)
        and b["image"] == b["image_apres"] == (q.IMAGE if c == "substitue" else IMAGE),
        "Artefacts",
    )
    exiger(
        b["magasin"]
        == (
            q.STORES["inactif"]
            if m == "inactif"
            else store["original" if c == "hors-db" else "copie"]
        ),
        "Magasin",
    )
    if m == "actif":
        exiger(
            json.loads(lire("variables.json"))
            == dict.fromkeys(("PK", "KEK", "db", "dbx"), True),
            "Variables après arrêt",
        )
    tr = lire("trace.log")
    exiger(
        b["stockage_apres"]
        == "2b07811057df887086f06a67edc6ebf911de8b6741156e7a2eb1416a4b8b1b2e"
        and not any(b["avant"]["interne"].values())
        and not any(b["apres"]["interne"].values())
        and all(
            not v["wr_operations"] and not v["wr_bytes"] for v in b["apres"].values()
        ),
        "E/S internes ou écriture",
    )
    exiger(
        b"vtd_dmar_enable enable 1" in tr[: b.get("debut_vcpu", len(tr))]
        and not re.search(
            rb"(?m)^(?:vtd_dmar_fault |pci_nvme_(?:io_cmd|admin_cmd|mmio_doorbell))", tr
        )
        and not re.search(rb"(?m)^vtd_reg_write ", tr[b.get("debut_vcpu", len(tr)) :]),
        "Activation ou changement VT-d",
    )
    ledger = Registre(b["reserve"], b["rids"]["xhci"], epoch)
    q.verif_tables(lire("initial.bin"), b, ledger)
    markers = [(27, 1), (27, 2), (29, 1), (27, 3)]
    nb = {
        "clone": 0,
        "quarantaine-vol": 0,
        "reconnexion": 1,
        "hors-db": 1,
        "substitue": 2,
    }.get(c, 4)
    exiger(
        lire("cpu.log").decode().splitlines()
        == ["initialise"]
        + [f"marqueur cpu=0 cellule={i} valeur={v}" for i, v in markers[:nb]],
        "Trajet du premier EFI",
    )
    if c in AVANT:
        exiger(b["decision"] == AVANT[c] and "verdict" not in b, "Verdict avant entrée")
    if c == "clone":
        exiger(
            len(b["origine_qmp"]) == 2
            and b["clone_sha256"] == IMAGE
            and not b["publications"],
            "Clone",
        )
        return epoch
    for loan in b["prets"]:
        ledger.confirmer(loan)
    if c in ("vol", "quarantaine-vol"):
        v = b["en_vol"]
        state = q.activite(tr[: v["position"]].decode().splitlines())
        exiger(
            v["soumis"] > v["clos"]
            and not state["anomalies"]
            and [list(row) for row in state["vols"]] == v["requetes"]
            and any(e > 1 and busy for _, e, busy in state["vols"])
            and v["decision"] == "refus de restitution",
            "Vol xHCI",
        )
    if c == "quarantaine-vol":
        q.verif_tables(lire("quarantaine.bin"), b, ledger)
        return epoch
    maps = lire("avant-load.bin")
    q.verif_tables(maps, b, ledger)
    pre = b["avant_load"]
    usb = pre["compteurs"]["usb5n"]
    exiger(
        usb["rd_bytes"] >= pre["octets"] == 33 << 20
        and usb["rd_operations"] > 0
        and not any(pre["compteurs"]["interne"].values()),
        "Chargement réel de l'ESP",
    )
    trace_load = tr[: pre["position"]]
    q.traductions(trace_load, b, ledger)
    usbstate = q.activite(trace_load.decode().splitlines())
    exiger(
        not usbstate["vols"]
        and not usbstate["anomalies"]
        and usbstate["luns"] == [0]
        and usbstate["adresses"] == [[1, "1"]],
        "Achèvement USB",
    )
    if c == "reconnexion":
        exiger(
            b["clone_sha256"] == IMAGE
            and not b["deconnecte"]
            and "ID: autre" in b["reconnecte"][0]
            and not any(b["apres"]["clone5n"].values())
            and "usb5n" not in b["apres"],
            "Reconnexion",
        )
        return epoch
    exiger(b["apres"]["usb5n"] == usb, "E/S USB postérieures")
    if c == "hors-db":
        exiger(
            b["efi_status"] in ((1 << 63) | 15, (1 << 63) | 26), "Refus du PE hors db"
        )
        return epoch
    if c == "substitue":
        exiger(
            b["pe"] != PE
            and set(b["sections"])
            == {".text", ".rdata", ".eh_fram", ".data", ".reloc"},
            "PE témoin",
        )
        return epoch
    exiger(
        lire("entree-remise.bin")
        == lire("transfert.bin") + lire("livraison.bin")
        == lire("arret-remise.bin"),
        "Remise différente entre le BDS, la première instruction et HLT",
    )
    co = remise(m, c, b, lire, tmp, precedent)
    actual = q.sections_chargees(pe, b["avant_start"]["base"])
    exiger(
        {n: list(v) for n, v in actual.items()} == co["sections"],
        "Cinq sections relogées",
    )
    exiger(
        lire("entree.bin") == maps == lire("apres.bin")
        and b["reads_before"] == b["apres"]
        and 0 < b["pile_entree"] < b["reserve"]
        and co["surface"][0] + co["surface"][1] < b["reserve"],
        "Continuité DMA",
    )
    windows = json.loads(lire("interruptions.json"))
    interval = lire("intervalle.log")
    apic = q.juger(tr, interval, windows)
    exiger(
        interval == tr[windows["debut"] : windows["fin"]]
        and apic == json.loads(lire("verdict-apic.json"))
        and apic["intervalle_admis"]
        and not re.search(
            rb"(?m)^(?:apm_io_write|usb_msd_cmd_submit|virtio_notify|vtd_reg_write|pci_nvme_)",
            interval,
        ),
        "Intervalle d'exécution",
    )
    r = json.loads(lire("rapport.json"))
    mark, addr, status, sb, arena, live, ep, digest = struct.unpack(
        "<16s5Q16s32s", lire("sortie.bin")
    )
    details = lire("details.bin")
    nom = status == 0
    exiger(
        mark == b"NOVA5M-RAM-FINAL"
        and ep.hex() == r["epoque"] == epoch
        and digest.hex() == r["pe_sha256"] == PE
        and status == r["statut"] == CAS[c]
        and sb == int(m == "actif")
        and r["secure_boot"] == m
        and r["allocations_vivantes"] == live == 0
        and 0 < arena == r["octets_arene"] <= 4 << 20
        and b["avant_start"]["base"]
        <= addr
        < b["avant_start"]["base"] + q.pe_info(pe)["taille"]
        and tuple(r[n] for n in CHAMPS) == struct.unpack("<12Q32s", details)[:-1]
        and r["plan"] == details[96:].hex()
        and not r["activation"]
        and r["activation_autorisee"] is False
        and r["contrat_sha256"] == empreinte(lire("transfert.bin"))
        and r["usb"] == int(nom or c in ("elf", "manifeste")),
        "Résultat en RAM",
    )
    exiger(
        r["origine"] == ("usb-authentifiee" if r["usb"] else "indeterminee")
        and b["decision"]
        == (
            "diagnostic-acheve"
            if nom
            else "refus-sans-surface"
            if c == "surface"
            else "refus-affiche"
        )
        and r["compatibles"] == r["conflits"] == int(nom)
        and r["pilotes_compatibles"] == (["org.novaos.lab.ioapic"] if nom else [])
        and r["conflits_selection"]
        == (["org.novaos.lab.hpet-a", "org.novaos.lab.hpet-b"] if nom else [])
        and r["plan"]
        == (
            empreinte(
                (DOSSIER.parent / "diagnostic-5e/vecteurs/ioapic.toml").read_bytes()
            )
            if nom
            else "00" * 32
        ),
        "Verdict ou sélection",
    )
    if r["usb"]:
        exiger(
            (r["port"], r["rid"], r["pages"])
            == (co["port"], co["rid"], len(ledger.feuilles())),
            "Origine reconnue",
        )
    if nom:
        exiger(
            0 < r["tables"]
            and 0 < r["pci"] <= 64
            and r["ioapic"] == r["hpet"] == r["drhd"] == 1,
            "Inventaire",
        )
    journal = lire("journal.txt").decode()
    exiger(
        f"statut {status}, origine {r['origine']}, Secure Boot {m}" in journal
        and "activation interdite" in journal,
        "Journal",
    )
    decoder_ecran(lire("avant.ppm"), lire("apres.ppm"), co["surface"], r)
    return epoch


def verifier(archive, fichier_pe):
    pe = fichier_pe.read_bytes()
    store = json.loads((BASE / "variables.json").read_bytes())
    exiger(
        empreinte(pe) == PE
        and store["ajouts"] == [q.pe_info(pe)["authenticode"]]
        and store["original"]
        == "bf41a49e08223f8e0a0192371d8fb79602faf56e4c0a58a5ad79624a54f2ed8f"
        and {k for k, v in store["variables"].items() if v["avant"] != v["apres"]}
        == {"db"},
        "Référence du banc",
    )
    names = {f"{m}-{c}/{p}" for m, c in cellules() for p in pieces(m, c)}
    epochs = set()
    with zipfile.ZipFile(archive) as z, tempfile.TemporaryDirectory() as temp:
        exiger(
            len(z.namelist()) == len(names)
            and set(z.namelist()) == names
            and not z.testzip(),
            "Liste des preuves",
        )
        for m, c in cellules():

            def lire(n, prefix=f"{m}-{c}/"):
                return z.read(prefix + n)

            prev = z.read(m + "-nominal/transfert.bin") if c == "reutilise" else None
            epoch = cellule(m, c, lire, pe, Path(temp), prev, store)
            exiger(epoch not in epochs, "Collision entre démarrages")
            epochs.add(epoch)
    print(
        f"{len(epochs)} démarrages distincts, {len(names)} pièces, signatures, tables VT-d, E/S et écrans vérifiés."
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    subs = p.add_subparsers(required=True, dest="cmd")
    a = subs.add_parser("archiver")
    for n in ("inactif", "actif", "sortie"):
        a.add_argument(n, type=Path)
    v = subs.add_parser("verifier")
    v.add_argument("archive", type=Path)
    v.add_argument("pe", type=Path)
    args = p.parse_args()
    if args.cmd == "archiver":
        archiver(args.inactif, args.actif, args.sortie)
    else:
        verifier(args.archive, args.pe)
