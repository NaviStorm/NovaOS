"""Archive sur liste fermée et contre-lecture indépendante sans accès à une VM."""

import argparse
from hashlib import sha256
import json
from pathlib import Path
import re
import struct
import sys
import tempfile
import zipfile

from observation import NOVA, activite, require
from temoin_usb import chemin_usb, garde, tampons, lecture_hotplug

sys.path.insert(0, str(NOVA / "boot/diagnostic-5o"))
from contrelecture import images, signe, legacy, juger
from image5o import chargeur
from demarrage import STORES
from image import sections_chargees
from archiver import MODES, SCENARIOS, PIECES

PROFILS = ("direct", "bochs", "double", "reconnexion", "en-vol", "hub")
IMAGE = "a4dd3d96739b0821f955a4e4e14524516b100e557d333739a6cecda23d31d7cb"
BASE = NOVA / "docs/validation/diagnostic-uefi-5p"


def digest(data):
    return sha256(data).hexdigest()


def anonymiser(trace):
    # Identités opaques d'objets du processus QEMU. Conserver les positions en octets.
    ids = {}

    def opaque(m):
        ptr = m[2]
        if ptr not in ids:
            ids[ptr] = len(ids) + 1
        replacement = f"{ids[ptr]:0{len(ptr) - 2}x}".encode()
        require(len(replacement) == len(ptr) - 2, "Identité synthétique trop longue")
        return m[1] + b"0x" + replacement

    return re.sub(
        rb"(usb_xhci_xfer_(?:start|async|success|error) |bus |state |child |blk |bs |ahci\()(0x[0-9a-f]+)",
        opaque,
        trace,
    )


def archiver(build, usb, vol, exclusion, out, reconnexion=None):
    with zipfile.ZipFile(out, "x") as z:

        def add(name, data):
            entry = zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0))
            entry.external_attr = 0o100644 << 16
            z.writestr(entry, data, zipfile.ZIP_DEFLATED, compresslevel=9)

        for mode in MODES:
            for profil in PROFILS:
                series = {"en-vol": vol, "reconnexion": reconnexion or usb}
                p = series.get(profil, usb) / (profil + "-" + mode)
                prefix = "usb/" + p.name + "/"
                for name in (
                    "bilan.json",
                    "integrite.json",
                    "firmware.log",
                    "transactions.log",
                ):
                    raw = (p / name).read_bytes()
                    add(
                        prefix + name,
                        anonymiser(raw) if name == "transactions.log" else raw,
                    )
                if profil != "hub":
                    for name in ("avant.ppm", "apres.ppm"):
                        add(prefix + name, (p / name).read_bytes())
            p = exclusion / mode
            for name in (
                "bilan.json",
                "tables.bin",
                "transactions.log",
                "firmware.log",
            ):
                add("exclusion/" + mode + "/" + name, (p / name).read_bytes())
            g = build / ("gop-" + mode)
            for cas in SCENARIOS:
                prefix = f"gop/{mode}/{cas}/"
                add(prefix + "bilan.json", (g / (cas + ".json")).read_bytes())
                for name in (*PIECES, "avant.ppm", "apres.ppm"):
                    add(prefix + name, (g / cas / name).read_bytes())
                fin = json.loads((g / cas / "interruptions.json").read_bytes())["fin"]
                add(prefix + "trace.log", (g / cas / "trace.log").read_bytes()[:fin])


def verifier_tables(b, tables, unit):
    root = unit["rtaddr"]
    require(
        root >= 1 << 32
        and unit["gsts"] & 0x80000000
        and len(tables) == 14 * 4096
        and digest(tables) == b["tables_sha256"],
        "Tables VT-d",
    )
    decoded = legacy(
        unit,
        {root + n: tables[n : n + 4096] for n in range(0, len(tables), 4096)},
        {root + i * 4096 for i in range(14, 18)},
    )
    require(
        {str(k): v for k, v in decoded.items()} == b["domaines"]
        and b["rids"]["xhci"] not in decoded
        and b["rids"]["stock"] in decoded,
        "Domaine du stockage ou xHCI",
    )
    return decoded


def verifier(path):
    artifact = chargeur()
    pe = digest(artifact)
    epochs = set()
    counts = dict(usb=0, hub=0, gop=0, exclusion=0)
    key = (NOVA / "boot/diagnostic-5m/autorite-laboratoire.pub").read_bytes()
    allowed = set()
    for mode in MODES:
        for profil in PROFILS:
            pieces = (
                "bilan.json",
                "integrite.json",
                "firmware.log",
                "transactions.log",
            )
            if profil != "hub":
                pieces += ("avant.ppm", "apres.ppm")
            allowed.update(f"usb/{profil}-{mode}/{n}" for n in pieces)
        allowed.update(
            f"exclusion/{mode}/{n}"
            for n in ("bilan.json", "tables.bin", "transactions.log", "firmware.log")
        )
        for cas in SCENARIOS:
            allowed.update(
                f"gop/{mode}/{cas}/{n}"
                for n in (*PIECES, "bilan.json", "avant.ppm", "apres.ppm", "trace.log")
            )
    with zipfile.ZipFile(path) as z, tempfile.TemporaryDirectory() as temp:
        require(
            len(z.namelist()) == len(allowed)
            and set(z.namelist()) == allowed
            and all(0 <= i.file_size <= 8 * 1024**2 for i in z.infolist())
            and sum(i.file_size for i in z.infolist()) < 160 * 1024**2
            and z.testzip() is None,
            "Archive ambiguë",
        )
        for mode in MODES:
            for profil in PROFILS:
                prefix = f"usb/{profil}-{mode}/"

                def piece(n):
                    return z.read(prefix + n)

                b = json.loads(piece("bilan.json"))
                integrite = json.loads(piece("integrite.json"))
                raw = piece("transactions.log")
                require(
                    b["mode"] == mode
                    and b["profil"] == profil
                    and b["epoque"] not in epochs
                    and integrite
                    == dict(avant=IMAGE, apres={"media.img": IMAGE, "clone.img": IMAGE})
                    and b["restitution"] is False
                    and b["magasin"]
                    == (
                        "bf41a49e08223f8e0a0192371d8fb79602faf56e4c0a58a5ad79624a54f2ed8f"
                        if mode == "actif"
                        else STORES["inactif"]
                    ),
                    "Identité ou intégrité",
                )
                epochs.add(b["epoque"])
                if profil == "hub":
                    nb = len(re.findall(rb"(?m)^ide_bus_exec_cmd ", raw))
                    require(
                        b["decision"] == "refus"
                        and b["echec_io"]
                        and b["commandes_sata"] == nb
                        and b["compteurs"]["interne"]["rd_operations"] > 0
                        and not b["compteurs"]["interne"]["wr_operations"]
                        and any(p == "1.2" for _, p in b["activite"]["adresses"]),
                        "Contre-exemple du hub",
                    )
                    counts["hub"] += 1
                    continue
                cut, end = b["trace_entree"], b["trace_fin"]

                # La sérialisation JSON représente les tuples Python par des listes.
                def jsonable(v):
                    return json.loads(json.dumps(v))

                req = activite(raw[:cut].decode().splitlines())
                require(
                    0 <= cut <= end <= len(raw)
                    and jsonable(req) == b["activite"]
                    and req["anomalies"] == 0
                    and req["luns"] == [0]
                    and b["pe"] == pe
                    and b["sections_lues"]
                    == jsonable(sections_chargees(artifact, b["base"])),
                    "Chronologie USB",
                )
                require(
                    not re.search(rb"(?m)^(ide_bus_exec_cmd|ahci_cmd_done) ", raw[:end])
                    and not re.search(
                        rb"(?m)^(vtd_reg_write|usb_msd_cmd_submit) ", raw[cut:end]
                    )
                    and b["apres"]
                    == (
                        b["changement"]["compteurs_nouvelle_incarnation"]
                        if profil == "reconnexion"
                        else b["avant"]
                    )
                    and not any(b["avant"]["interne"].values())
                    and not any(b["apres"]["interne"].values()),
                    "E/S internes",
                )
                require(
                    b["ecran"] == b["ecran_apres"]
                    and digest(piece("avant.ppm")) == b["ecran"][2]
                    and piece("apres.ppm") == piece("avant.ppm")
                    and b["verdict"]["statut"] == 16
                    and b["verdict"]["confiance"] == 2
                    and b["verdict"]["vivants"] == 0,
                    "Arrêt USB",
                )
                ident = chemin_usb(
                    b["inventaire"],
                    b["loaded_image"],
                    b["origine"],
                    b["gpt"],
                    b["liaison_qom"],
                )
                require(
                    (profil == "double") == (len(b["sources"]) == 2)
                    and b["avant"]["usb5n"]["rd_bytes"] > 0
                    and b["avant"]["usb5n"]["wr_bytes"] == 0
                    and b["origine"].encode() in piece("firmware.log")
                    and jsonable(
                        tampons(
                            raw[:cut],
                            b["inventaire"]["ram"],
                            b["base"],
                            b["taille_image"],
                        )
                    )
                    == b["tampons"],
                    "Identification de l'image ou des buffers",
                )
                v = b["vol"]
                c = b["changement"]
                if profil == "en-vol":
                    require(
                        v["etat_qmp"] == "running"
                        and v["epoque"] == b["epoque"]
                        and 0 < v["position"] < cut
                        and jsonable(
                            activite(raw[: v["position"]].decode().splitlines())
                        )
                        == v["activite"]
                        and any(
                            ep > 1 and pending
                            for _, ep, pending in v["activite"]["vols"]
                        ),
                        "Transfert en vol",
                    )
                if profil == "reconnexion":
                    require(
                        b["avant"]["usb5n"]["rd_bytes"] > 0
                        and not any(b["apres"]["clone5n"].values())
                        and lecture_hotplug(
                            raw, cut, end, c["debut_ajout"], c["fin_ajout"]
                        )
                        == c["lecture_modele_octets"]
                        and c["generation_apres"] == c["generation_avant"] + 1
                        and any("ID: remplacement" in s for s in c["apres"])
                        and not any("ID: cle1" in s for s in c["apres"]),
                        "Réemploi du port",
                    )
                require(
                    b["temoin"] == garde(b["epoque"], ident, b["sources"], c, v),
                    "Décision du témoin",
                )
                counts["usb"] += 1
            prefix = f"exclusion/{mode}/"
            b = json.loads(piece("bilan.json"))
            raw = piece("transactions.log")
            verifier_tables(b, piece("tables.bin"), b["unite"])
            rid = b["rids"]["xhci"]
            faults = re.findall(
                rf"(?m)^vtd_dmar_fault sid {rid:#x} fault \d+ addr 0x[0-9a-f]+ write [01]$",
                raw.decode(),
            )
            require(
                b["mode"] == mode
                and b["epoque"] not in epochs
                and b["unite"] == b["unite_fin"]
                and b["avant"] == b["apres"]
                and not any(
                    n for device in b["apres"].values() for n in device.values()
                )
                and faults
                and faults == b["fautes_xhci"]
                and b"usb_msd_cmd_submit " not in raw,
                "Exclusion au reset",
            )
            epochs.add(b["epoque"])
            counts["exclusion"] += 1
            for cas in SCENARIOS:
                prefix = f"gop/{mode}/{cas}/"
                b = json.loads(piece("bilan.json"))
                v = juger(
                    piece("trace.log"),
                    piece("intervalle.log"),
                    json.loads(piece("interruptions.json")),
                )
                require(
                    v["intervalle_admis"]
                    and v["eoi_attribues"] == b["eoi_attribues"]
                    and not any(b["stockage"].values()),
                    "Intervalle du GOP",
                )
                rec = piece("recu.bin")
                bail = piece("surface.bin")
                require(
                    rec[48:80].hex() == pe
                    and rec[32:48].hex() not in epochs
                    and signe(rec, key, Path(temp))
                    and digest(rec) == b["recu_sha256"]
                    and digest(bail) == b["bail_sha256"],
                    "Reçu primaire",
                )
                epochs.add(rec[32:48].hex())
                if cas == "absent":
                    require(bail == bytes(168), "Bail absent")
                else:
                    require(
                        (cas != "signature") == signe(bail, key, Path(temp))
                        and bail[32:64] == rec[48:80]
                        and (cas != "epoque") == (bail[16:32] == rec[32:48]),
                        "Bail graphique",
                    )
                _, _, code, trust, arena, live, epoch, ident = struct.unpack(
                    "<16s5Q16s32s", piece("sortie.bin")
                )
                require(
                    (code, trust, arena, live, epoch, ident)
                    == (
                        b["verdict"],
                        int(mode == "actif"),
                        b["arena"],
                        0,
                        rec[32:48],
                        rec[48:80],
                    ),
                    "Sortie RAM",
                )
                if cas == "nominal":
                    require(
                        list(struct.unpack_from("<QQIIII", bail, 64)) == b["surface"],
                        "Rectangle et surface signés",
                    )
                require(
                    b["confiance"] == int(mode == "actif")
                    and b["verdict"]
                    == {
                        "nominal": 40,
                        "absent": 41,
                        "epoque": 41,
                        "signature": 42,
                        "desaccord": 44,
                        "limites": 44,
                    }[cas],
                    "Verdict graphique",
                )
                verifier_tables(b, piece("tables.bin"), json.loads(piece("unite.json")))
                images(cas, b, piece("avant.ppm"), piece("apres.ppm"))
                counts["gop"] += 1
    return counts


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="action", required=True)
    a = sub.add_parser("archiver")
    for name in ("build", "usb", "vol", "exclusion", "sortie"):
        a.add_argument(name, type=Path)
    a.add_argument("--reconnexion", type=Path)
    sub.add_parser("verifier").add_argument(
        "archive", type=Path, nargs="?", default=BASE / "observations.zip"
    )
    a = p.parse_args()
    if a.action == "archiver":
        archiver(a.build, a.usb, a.vol, a.exclusion, a.sortie, a.reconnexion)
    else:
        print(json.dumps(verifier(a.archive), sort_keys=True))
