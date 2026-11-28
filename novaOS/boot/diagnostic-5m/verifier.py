"""Contre-lecture des archives publiques sans lancer les exécutables EFI."""

import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tarfile
import uuid
from tempfile import TemporaryDirectory
from image import check, pe_info, sections_chargees

BASE = Path(__file__).resolve().parent
NOVA = BASE.parents[1]
sys.path.extend([str(NOVA / "boot/diagnostic-5i"), str(NOVA / "boot/diagnostic-5k")])
from domaines import legacy
from verdict_apic import juger

ORIGINE = b"NovaOS/5M/OVMF-5J/ramdisk/EFI/BOOT/BOOTX64.EFI\0"
RESULTATS = {
    "valide": 0,
    "altere": 25,
    "signataire": 25,
    "reserve": 26,
    "absent": 23,
    "perime": 24,
    "manifeste": 6,
    "paquet": 18,
}
MODES = ("actif", "inactif")


def sha(b):
    return sha256(b).hexdigest()


def signature(record, cle, travail):
    check(len(record) == 176 and len(cle) == 32, "Taille Ed25519")
    (travail / "cle.der").write_bytes(bytes.fromhex("302a300506032b6570032100") + cle)
    (travail / "corps.bin").write_bytes(record[:112])
    (travail / "sig.bin").write_bytes(record[112:])
    res = subprocess.run(
        [
            "openssl",
            "pkeyutl",
            "-verify",
            "-pubin",
            "-inkey",
            str(travail / "cle.der"),
            "-keyform",
            "DER",
            "-rawin",
            "-in",
            str(travail / "corps.bin"),
            "-sigfile",
            str(travail / "sig.bin"),
        ],
        capture_output=True,
    )
    if res.returncode == 0:
        return True
    check(
        res.returncode == 1 and b"Signature Verification Failure" in res.stdout,
        "Vérificateur OpenSSL indisponible",
    )
    return False


def comptages(raw, transition):
    rows = [{x["device"]: x["stats"] for x in json.loads(l)} for l in raw.splitlines()]
    check(len(rows) == (2 if transition else 1), "Nombre de clichés QMP")
    for d in rows:
        check(
            all(
                v == 0
                for k, v in d["interne"].items()
                if k.endswith(("_operations", "_bytes"))
            ),
            "E/S internes",
        )
    for dev in ("pflash0", "pflash1"):
        for k in ("wr_bytes", "wr_operations", "flush_operations"):
            check(
                rows[0][dev][k] == rows[-1][dev][k],
                "Écriture flash pendant le diagnostic",
            )


def verifier_intervalle(r, pe, cal, get):
    tr = get("trace.log")
    inter = get("intervalle.log")
    proof = json.loads(get("interruptions.json"))
    verdict = juger(tr, inter, proof)
    check(
        verdict == json.loads(get("verdict-apic.json"))
        and verdict["intervalle_admis"]
        and verdict["eoi_attribues"] == r["eoi"] >= 1,
        "Verdict APIC",
    )
    check(
        not any(
            x in inter
            for x in (
                b"apm_io_write",
                b"SMM:",
                b"pci_nvme_io_cmd",
                b"vtd_dmar_fault",
                b"vtd_inv_desc",
                b"virtio_notify",
            )
        ),
        "Activité interdite",
    )
    e = r["entree"]
    info = pe_info(pe)
    base = e["base"]
    rva, off, _ = info["texte"]
    i = off + info["entree"] - rva
    check(
        proof["entree"] == cal["entree"] == e["rip"] == base + info["entree"]
        and e["octets"] == pe[i : i + 16].hex()
        and proof["hlt"] == base + info["hlt"]
        and proof["sha_diagnostic"] == sha(pe)
        and base == cal["image"]
        and [proof["timer"]] == cal["timers"]
        and e["get_variable"] > 0,
        "Premier RIP et arrêt",
    )
    check(
        e["sections"]
        == {n: list(row) for n, row in sections_chargees(pe, base).items()},
        "Sections relocalisées",
    )
    check(r["pci_avant"] == r["pci_apres"], "État PCI/VT-d")
    (unit,) = r["pci_avant"]["unites"].values()
    root = unit["rtaddr"]
    tables = get("tables-avant.bin")
    pages = {root + i: tables[i : i + 4096] for i in range(0, len(tables), 4096)}
    decoded = legacy(unit, pages, {root + i * 4096 for i in range(14, 18)})
    check(
        {str(sid): v for sid, v in decoded.items()} == r["domaines"]
        and len(decoded) == 3
        and sorted(
            (d["did"], [m["droits"] for m in d["mappings"]]) for d in decoded.values()
        )
        == [(1, [1]), (2, [1]), (3, [1, 2])],
        "Domaines ou permissions",
    )
    fw = get("efi.log")
    check(
        fw.count(b"NOVA 5J transfert=")
        == fw.count(b"NOVA PIO LoadImage=0000000000000000")
        == 1
        and fw.count(
            b"NOVA 5J SecureBoot=" + f"{int(r['mode'] == 'actif'):016x}".encode()
        )
        == 1,
        "Admissibilité firmware",
    )


def verifier_db(original, copie, hashes):
    """Reconstitue toute la flash attendue à partir de la seule addition autorisée."""
    check(
        len(hashes) == len(set(hashes)) == 3
        and sha(original)
        == "e04b60150458454c7f9608209ac7358058c2bb566ddf254e455330c065b1411b",
        "Base OVMF 5J",
    )
    store = struct.unpack_from("<H", original, 48)[0]
    fin = store + struct.unpack_from("<I", original, store + 16)[0]
    motif = uuid.UUID("d719b2cb-3d3a-4596-a3bc-dad00e67656f").bytes_le + "db\0".encode(
        "utf-16le"
    )
    check(original.count(motif) == 1, "Variable db ambiguë")
    guid = original.index(motif)
    taille = struct.unpack_from("<I", original, guid - 4)[0]
    insert = guid + len(motif) + taille
    owner = uuid.UUID("45434150-cdbe-4c63-8fcd-a45fa42834df").bytes_le
    payload = b"".join(owner + bytes.fromhex(h) for h in hashes)
    liste = (
        uuid.UUID("c1c41626-504c-4092-aca9-41f936934328").bytes_le
        + struct.pack("<III", 28 + len(payload), 0, 48)
        + payload
    )
    check(
        insert < fin - len(liste)
        and original[fin - len(liste) : fin] == bytes([255]) * len(liste),
        "Capacité du volume",
    )
    attendu = bytearray(original)
    struct.pack_into("<I", attendu, guid - 4, taille + len(liste))
    attendu[insert:fin] = liste + original[insert : fin - len(liste)]
    check(attendu == copie, "Modification hors de la liste des empreintes db")


def verifier(dossier):
    for ligne in (dossier / "SHA256SUMS").read_text().splitlines():
        h, n = ligne.split()
        check(
            Path(n).name == n and sha((dossier / n).read_bytes()) == h, "Empreinte " + n
        )
    with tarfile.open(dossier / "supports.tar.gz") as a:
        supports = {m.name: a.extractfile(m).read() for m in a}
    ids = json.loads(supports["images.json"])
    check(
        supports["images.json"] == (BASE / "images-laboratoire.json").read_bytes(),
        "Versions",
    )
    tous = {nom: pe_info(supports[nom + ".efi"]) for nom in (*ids, "hors-db")}
    check(all(v == tous[n] for n, v in ids.items()), "Identités PE")
    magasin = json.loads(supports["magasin.json"])
    variables = magasin["variables"]
    check(
        magasin["original"]
        == "e04b60150458454c7f9608209ac7358058c2bb566ddf254e455330c065b1411b"
        and magasin["copie"] == sha(supports["actif.fd"])
        and sha(supports["inactif.fd"])
        == "8c09b326b45951f539619b4d0f33e5e8fdbafa1cca9b173468f9df7e4a7725e0"
        and sorted(magasin["ajouts"]) == sorted(x["authenticode"] for x in ids.values())
        and tous["hors-db"]["authenticode"] not in magasin["ajouts"]
        and {n for n, x in variables.items() if x["avant"] != x["apres"]} == {"db"}
        and all(
            x["horodatage_avant"] == x["horodatage_apres"] for x in variables.values()
        ),
        "Périmètre db",
    )
    with tarfile.open(
        NOVA / "docs/validation/diagnostic-uefi-5j/laboratoire.tar.gz"
    ) as j:
        verifier_db(
            j.extractfile("supports5j-b/nominal.fd").read(),
            supports["actif.fd"],
            magasin["ajouts"],
        )
    cle = (BASE / "autorite-laboratoire.pub").read_bytes()
    namespace = sha(sha256(b"NovaOS/5M/namespace").digest() * 131072)
    resumes = []
    epoques = set()
    with (
        tarfile.open(dossier / "observations.tar.gz") as archive,
        TemporaryDirectory(dir=os.environ.get("NOVAOS_TMPDIR")) as td,
    ):

        def lire(n):
            return archive.extractfile(n).read()

        for mode in MODES:
            cal = json.loads(lire("cal-" + mode + ".json"))
            for cas in (*RESULTATS, "reutilise", "substitution") + (
                ("refus-db",) if mode == "actif" else ()
            ):

                def get(f):
                    return lire(mode + "-" + cas + "/" + f)

                r = json.loads(get("resultat.json"))
                media = json.loads(get("media.json"))
                rec, ancre = get("transfert.bin"), get("epoch.bin")
                nom = {
                    "substitution": "paquet",
                    "refus-db": "hors-db",
                    "manifeste": "manifeste",
                    "paquet": "paquet",
                }.get(cas, "valide")
                pe = supports[nom + ".efi"]
                fat = supports[nom + ".img"]
                check(
                    fat.count(pe) == 1
                    and media["position"] == fat.index(pe)
                    and media["taille"] == len(pe)
                    and media["efi"] == sha(pe)
                    and media["sha256"]
                    == sha(fat + bytes((34 << 20) - len(fat) - 176) + rec),
                    "Support du démarrage",
                )
                check(
                    (r["mode"], r["cas"]) == (mode, cas)
                    and r["magasin"] == sha(supports[mode + ".fd"])
                    and r["epoque"] not in epoques
                    and r["namespace_avant"] == r["namespace_apres"] == namespace,
                    "Instance ou stockage",
                )
                epoques.add(r["epoque"])
                check(
                    len(get("tables-avant.bin")) == 14 * 4096
                    and get("tables-avant.bin") == get("tables-apres.bin"),
                    "Tables changées",
                )
                actif = cas in RESULTATS
                comptages(get("compteurs-qmp.jsonl"), actif)
                if cas == "absent":
                    check(rec == bytes(176), "Absence du reçu")
                else:
                    check(
                        rec[:16] == b"NOVA5M-TRUST-V1!"
                        and rec[16:20] == bytes((int(mode == "actif"), 0, 255, 1))
                        and (rec[80:112] == cle) == (cas != "signataire")
                        and rec[28:32] == bytes((int(cas == "reserve"), 0, 0, 0))
                        and signature(rec, rec[80:112], Path(td)) == (cas != "altere")
                        and (cas != "signataire" or not signature(rec, cle, Path(td))),
                        "Authenticité du reçu",
                    )
                    check(
                        (sha256(ORIGINE + rec[:112]).digest() == ancre)
                        == (cas != "perime"),
                        "Engagement",
                    )
                    if cas in ("perime", "reutilise"):
                        check(
                            rec == lire(mode + "-valide/transfert.bin")
                            and rec[32:48].hex() != r["epoque"]
                            and (ancre == lire(mode + "-valide/epoch.bin"))
                            == (cas == "reutilise"),
                            "Rejeu",
                        )
                    else:
                        check(rec[32:48].hex() == r["epoque"], "Époque")
                    check(
                        rec[48:80].hex() == r["image_attendue"]
                        and (r["image_attendue"] == media["efi"])
                        == (cas not in ("substitution", "refus-db")),
                        "Liaison au PE",
                    )
                if actif:
                    verifier_intervalle(r, pe, cal, get)
                    marque, adr, statut, sb, mem, live, epoch, img = struct.unpack(
                        "<16s5Q16s32s", get("sortie.bin")
                    )
                    valide = cas in ("valide", "paquet", "manifeste")
                    check(
                        marque == b"NOVA5M-RAM-FINAL"
                        and r["cpu_libere"]
                        and statut == RESULTATS[cas]
                        and live == 0
                        and sb == (int(mode == "actif") if valide else 2)
                        and epoch == (rec[32:48] if valide else bytes(16))
                        and img == (rec[48:80] if valide else bytes(32))
                        and r["sortie"]
                        == dict(
                            adresse=adr,
                            statut=statut,
                            confiance=sb,
                            arena=mem,
                            vivants=live,
                            epoque=epoch.hex(),
                            image=img.hex(),
                        ),
                        "Reçu terminal",
                    )
                    if cas == "valide":
                        check(
                            r["seconde_admission_meme_vm"] == "refusee",
                            "Double admission",
                        )
                elif cas == "refus-db":
                    check(
                        r["refus_firmware"] == "EFI_ACCESS_DENIED"
                        and b"NOVA PIO LoadImage=800000000000000f" in get("efi.log")
                        and b"NOVA 5J transfert=" not in get("efi.log")
                        and r["premier_efi"] is False,
                        "Refus par OVMF",
                    )
                else:
                    check(
                        not r["cpu_libere"]
                        and r["refus_avant_chargement"]
                        and get("efi.log") == b"",
                        "Refus avant instructions",
                    )
                resumes.append(
                    (mode, cas, r.get("eoi", 0), r.get("sortie", {}).get("statut"))
                )
    return resumes


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument(
        "dossier",
        nargs="?",
        type=Path,
        default=NOVA / "docs/validation/diagnostic-uefi-5m",
    )
    rows = verifier(p.parse_args().dossier)
    for row in rows:
        print(*row)
    print(
        f"{len(rows)} décisions, {sum(x[3] is not None for x in rows)} transitions EFI, {sum(x[2] for x in rows)} EOI."
    )
