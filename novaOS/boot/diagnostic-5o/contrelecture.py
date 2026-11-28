"""Contre-lecture hors VM des preuves 5O. OpenSSL 3 vérifie les deux reçus."""

import gzip
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import zipfile
from archiver import MODES, SCENARIOS, png
from image5o import PREUVES, ROOT, MIO, chargeur, lecteur, verifier, require
from demarrage import STORES, public_member
from image import pe_info

sys.path.extend(str(ROOT / "boot" / ("diagnostic-5" + p)) for p in ("i", "k"))
from domaines import legacy
from verdict_apic import juger


def hash(b):
    return hashlib.sha256(b).hexdigest()


def signe(blob, key, work):
    (work / "public.der").write_bytes(bytes.fromhex("302a300506032b6570032100") + key)
    (work / "corps").write_bytes(blob[:-64])
    (work / "sig").write_bytes(blob[-64:])
    p = subprocess.run(
        [
            "openssl",
            "pkeyutl",
            "-verify",
            "-pubin",
            "-keyform",
            "DER",
            "-inkey",
            str(work / "public.der"),
            "-rawin",
            "-in",
            str(work / "corps"),
            "-sigfile",
            str(work / "sig"),
        ],
        capture_output=True,
    )
    require(
        p.returncode == 0
        or (p.returncode == 1 and b"Signature Verification Failure" in p.stdout),
        "OpenSSL 3",
    )
    return p.returncode == 0


def images(cas, b, initial, final):
    require(
        hash(initial) == b["avant_ppm"] and hash(final) == b["apres_ppm"],
        "Capture divergente",
    )
    a = initial.split(b"\n", 3)
    z = final.split(b"\n", 3)
    w, h = b["surface"][2:4]
    require(
        a[:3] == z[:3] == [b"P6", f"{w} {h}".encode(), b"255"]
        and len(a[3]) == len(z[3]) == w * h * 3,
        "Géométrie",
    )
    if cas != "nominal":
        require(initial == final and not b["rendu"], "Pixels avec bail refusé")
        return
    require(
        initial != final and b["rendu"] and b["octets_fenetre"] == 544 * 120 * 4,
        "Verdict graphique absent",
    )
    for y in range(h):
        offset = 3 * (w * y + (544 if y < 120 else 0))
        require(
            a[3][offset : 3 * w * (y + 1)] == z[3][offset : 3 * w * (y + 1)],
            "Changement hors rectangle",
        )


def valider():
    pe = chargeur()
    key = (ROOT / "boot/diagnostic-5m/autorite-laboratoire.pub").read_bytes()
    resultats = {
        "nominal": 40,
        "absent": 41,
        "signature": 42,
        "epoque": 41,
        "limites": 44,
        "desaccord": 44,
    }
    magasins = json.loads((PREUVES / "variables.json").read_text())
    require(
        magasins["ajouts"] == [pe_info(pe)["authenticode"]]
        and hash(public_member("actif.fd", STORES["actif"])) == magasins["original"]
        and hash(gzip.decompress((PREUVES / "magasin-ovmf-test.fd.gz").read_bytes()))
        == magasins["copie"]
        and all(
            v["avant"] == v["apres"]
            for n, v in magasins["variables"].items()
            if n != "db"
        ),
        "Magasins",
    )
    with (
        tempfile.TemporaryDirectory() as tmp,
        zipfile.ZipFile(PREUVES / "observations.zip") as archive,
    ):
        work = Path(tmp)
        with (work / "media.img").open("xb") as f:
            f.write(
                gzip.decompress((PREUVES / "novaos-x86_64-68mio.img.gz").read_bytes())
            )
        with lecteur(work / "media.img") as f:
            verifier(f, 68 * MIO, pe)
            f.seek(0)
            require(
                hash(f.read())
                == json.loads((PREUVES / "capacites.json").read_text())["capacites"][0][
                    "sha256"
                ],
                "Image distribuée",
            )
        require(
            archive.testzip() is None
            and len(archive.namelist()) == len(set(archive.namelist())),
            "Archive",
        )
        epochs = set()
        for mode in MODES:
            for cas in SCENARIOS:

                def piece(n):
                    return archive.read(f"{mode}/{cas}/{n}")

                b = json.loads(piece("bilan.json"))
                proof = json.loads(piece("interruptions.json"))
                io = juger(piece("trace.log"), piece("intervalle.log"), proof)
                require(
                    proof["sha_diagnostic"] == hash(pe)
                    and io["intervalle_admis"]
                    and io["eoi_attribues"] == b["eoi_attribues"],
                    "Interruptions",
                )
                require(
                    not any(
                        t in piece("intervalle.log")
                        for t in (
                            b"SMM:",
                            b"apm_io_write",
                            b"vtd_dmar_fault",
                            b"pci_nvme_",
                        )
                    ),
                    "E/S interdites",
                )
                rec = piece("recu.bin")
                bail = piece("surface.bin")
                require(
                    len(rec) == 176
                    and signe(rec, key, work)
                    and rec[32:48] not in epochs
                    and rec[48:80] == hashlib.sha256(pe).digest(),
                    "Confiance primaire",
                )
                epochs.add(rec[32:48])
                require(
                    len(bail) == 168
                    and hash(bail) == b["bail_sha256"]
                    and hash(rec) == b["recu_sha256"],
                    "Identité des reçus",
                )
                if cas == "absent":
                    require(bail == bytes(168), "Bail attendu absent")
                else:
                    require(
                        signe(bail, key, work) == (cas != "signature")
                        and bail[32:64] == rec[48:80]
                        and (bail[16:32] == rec[32:48]) == (cas != "epoque"),
                        "Engagement écran",
                    )
                    if cas == "nominal":
                        require(
                            list(struct.unpack_from("<QQIIII", bail, 64))
                            == b["surface"],
                            "Étendue du bail nominal",
                        )
                _, _, code, trust, arena, live, epoch, ident = struct.unpack(
                    "<16s5Q16s32s", piece("sortie.bin")
                )
                require(
                    code == b["verdict"] == resultats[cas]
                    and trust == int(mode == "actif")
                    and live == 0
                    and epoch == rec[32:48]
                    and ident == rec[48:80]
                    and arena == b["arena"],
                    "Sortie de l’EFI",
                )
                unit = json.loads(piece("unite.json"))
                root = unit["rtaddr"]
                tables = piece("tables.bin")
                require(
                    len(tables) == 14 * 4096 and hash(tables) == b["tables_sha256"],
                    "Pages de traduction",
                )
                mapping = legacy(
                    unit,
                    {
                        root + i * 4096: tables[i * 4096 : (i + 1) * 4096]
                        for i in range(14)
                    },
                    {root + i * 4096 for i in range(14, 18)},
                )
                require(
                    root >= 1 << 32
                    and {str(k): v for k, v in mapping.items()} == b["domaines"]
                    and b["rids"]["xhci"] not in mapping
                    and not any(b["stockage"].values()),
                    "Isolation Q35",
                )
                images(cas, b, piece("avant.ppm"), piece("apres.ppm"))
                if cas == "nominal":
                    require(
                        png(piece("apres.ppm"))
                        == (PREUVES / f"verdict-{mode}.png").read_bytes(),
                        "Projection PNG",
                    )
        usb = json.loads(archive.read("usb/bilan.json"))
        require(len(usb) == 11, "Nombre de démarrages USB")
        for k, v in usb.items():
            if k == "hors-db-actif-68":
                c = v["compteurs_terminal"]["interne"]
                require(
                    v["decision"] == "EFI_ACCESS_DENIED"
                    and not v["lancement"]
                    and v["temoin"] == hash(pe)
                    and c["rd_operations"] == 40
                    and c["rd_bytes"] == 32768
                    and c["wr_operations"] == 0,
                    "Refus original",
                )
            else:
                require(
                    archive.read(f"usb/{k}/avant.ppm")
                    == archive.read(f"usb/{k}/apres.ppm"),
                    "Affichage USB",
                )
                if k.startswith("ambigu-"):
                    require("clone5n" in v["compteurs_hlt"], "Contrôle du clone")
                require(
                    v["pe"] == hash(pe)
                    and v["verdict"]["statut"] == 16
                    and v["verdict"]["confiance"] == 2
                    and v["compteurs_entree"] == v["compteurs_hlt"]
                    and not v["trace_commandes_sata_octets"]
                    and v["supports_usb"] == (2 if k.startswith("ambigu") else 1)
                    and v["lecture_seule"]
                    and v["ecran_immobile_du_premier_rip_au_hlt"]
                    and v["interne_intact"],
                    "Démarrage USB",
                )
        j = json.loads(archive.read("anciens/bilan.json"))
        require(
            j["avant"] == j["apres"]
            and [s for s, _ in j["avant"]] == ["valide", "interrompu"] + ["libre"] * 6
            and j["demarrage"]["compteurs_entree"] == j["demarrage"]["compteurs_hlt"],
            "Conservation des journaux",
        )
    print(
        "12 verdicts GOP, 2 projections, 11 cas USB, 1 démarrage avec journaux, 2 magasins et image 68 Mio vérifiés."
    )


if __name__ == "__main__":
    valider()
