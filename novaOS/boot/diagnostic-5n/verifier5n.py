"""Contre-lecture hors VM des structures sur disque et des décisions du banc."""

import argparse
import gzip
import io
import json
from pathlib import Path
import re
import struct
import uuid
import zipfile
import zlib

from support import OS, PE_SHA, empreinte, notices, require, verify

PROOF = OS / "docs/validation/diagnostic-uefi-5n"


def geometry(data):
    s = 512
    total = len(data) // s
    require(
        total * s == len(data)
        and data[450] == 0xEE
        and struct.unpack_from("<II", data, 454) == (1, total - 1)
        and data[510:512] == b"\x55\xaa",
        "MBR",
    )
    tables = []
    for lba, other, expected_table in ((1, total - 1, 2), (total - 1, 1, total - 33)):
        h = bytearray(data[lba * s : lba * s + 92])
        require(
            h[:8] == b"EFI PART" and struct.unpack_from("<II", h, 8) == (65536, 92),
            "En-tête GPT",
        )
        (crc,) = struct.unpack_from("<I", h, 16)
        h[16:20] = bytes(4)
        require(
            zlib.crc32(h) == crc
            and struct.unpack_from("<4Q", h, 24) == (lba, other, 34, total - 34),
            "CRC ou bornes GPT",
        )
        pos, n, length, crc = struct.unpack_from("<QIII", h, 72)
        require((pos, n, length) == (expected_table, 128, 128), "Table GPT hors profil")
        table = data[pos * s : pos * s + n * length]
        require(zlib.crc32(table) == crc, "CRC des entrées")
        tables.append(table)
    require(
        tables[0] == tables[1] and not any(tables[0][256:]),
        "Miroir ou entrée supplémentaire",
    )
    result = {}
    previous = 33
    types = (
        "c12a7328-f81f-11d2-ba4b-00a0c93ec93b",
        "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7",
    )
    for i, expected in enumerate(("NOVAOS", "NOVALOGS")):
        t, _, first, last, attr, text = struct.unpack_from(
            "<16s16sQQQ72s", tables[0], i * 128
        )
        name = text.decode("utf-16-le").rstrip("\0")
        require(
            str(uuid.UUID(bytes_le=t)) == types[i]
            and name == expected
            and previous < first <= last <= total - 34
            and attr == 0,
            "Partition ambiguë",
        )
        result[name] = (first * s, (last - first + 1) * s)
        previous = last
    return result


def fat_files(data, extent, name):
    start, size = extent
    b = memoryview(data)[start : start + size]
    ss, cluster, reserved, fats = struct.unpack_from("<HBHB", b, 11)
    total, fat = struct.unpack_from("<2I", b, 32)
    (root,) = struct.unpack_from("<I", b, 44)
    require(
        (ss, cluster, fats, root) == (512, 1, 2, 2)
        and total * ss == size
        and bytes(b[71:82]).rstrip() == name.encode()
        and b[:ss] == b[6 * ss : 7 * ss]
        and b[ss : 2 * ss] == b[7 * ss : 8 * ss]
        and b[510:512] == b"\x55\xaa",
        "BPB ou sauvegarde",
    )
    base = reserved + fats * fat
    count = total - base
    require(count >= 65525 and fat * ss >= 4 * (count + 2), "Type FAT32")
    table = b[reserved * ss : (reserved + fat) * ss]
    require(table == b[(reserved + fat) * ss : base * ss], "Deux FAT différentes")
    used = set()

    def chain(c):
        result = bytearray()
        while True:
            require(2 <= c <= count + 1 and c not in used, "Cycle ou chaîne croisée")
            used.add(c)
            pos = (base + c - 2) * ss
            result.extend(b[pos : pos + ss])
            c = struct.unpack_from("<I", table, c * 4)[0] & 0xFFFFFFF
            if c >= 0xFFFFFF8:
                return result

    files = {}

    def walk(c, prefix):
        directory = chain(c)
        longname = None
        for at in range(0, len(directory), 32):
            e = directory[at : at + 32]
            if e[0] == 0:
                break
            if e[11] == 15:
                require(e[0] == 0x41 and longname is None, "Nom long inattendu")
                longname = (
                    (e[1:11] + e[14:26] + e[28:32]).decode("utf-16-le").split("\0")[0]
                )
                continue
            if e[11] == 8 or e[0] == ord("."):
                continue
            short = e[:8].decode("ascii").rstrip()
            ext = e[8:11].decode("ascii").rstrip()
            path = prefix + (longname or short + ("." + ext if ext else ""))
            longname = None
            first = (
                struct.unpack_from("<H", e, 20)[0] << 16
                | struct.unpack_from("<H", e, 26)[0]
            )
            if e[11] & 16:
                require(prefix.count("/") < 4, "Profondeur de répertoire")
                walk(first, path + "/")
            else:
                (length,) = struct.unpack_from("<I", e, 28)
                content = chain(first)
                require(
                    path not in files and len(content) - ss < length <= len(content),
                    "Étendue du fichier",
                )
                files[path] = bytes(content[:length])

    walk(root, "")
    (free,) = struct.unpack_from("<I", b, ss + 488)
    require(free == count - len(used), "FSInfo")
    return files


def verify_delivery(root=PROOF):
    raw = gzip.decompress((root / "novaos-x86_64.img.gz").read_bytes())
    parts = geometry(raw)
    require(set(parts) == {"NOVAOS", "NOVALOGS"}, "Deux volumes")
    esp = fat_files(raw, parts["NOVAOS"], "NOVAOS")
    require(
        set(esp) == {"EFI/BOOT/BOOTX64.EFI", "LICENCES.TXT"}
        and esp["LICENCES.TXT"] == notices()
        and empreinte(esp["EFI/BOOT/BOOTX64.EFI"]) == PE_SHA,
        "Chargeur 5M",
    )
    logs = fat_files(raw, parts["NOVALOGS"], "NOVALOGS")
    names = {
        f"BOOT{i:04d}/{n}"
        for i in range(1, 9)
        for n in ("rapport.json", "JOURNAL.TXT", "VALIDES.SHA")
    }
    require(
        set(logs) == names and all(not any(v) for v in logs.values()),
        "Slots préalloués",
    )
    obs = json.loads((root / "captures/observations.json").read_text())
    require(
        empreinte(raw) == obs["image"] == obs["copies"]["grand-dd"]["source"]
        and all(d["prefixe_identique"] for d in obs["copies"].values()),
        "Distribution dd",
    )
    phases = {
        "standard": 10,
        "grand-dd": 2,
        "grand-ajuste": 1,
        "sans-logs": 1,
        "fat-alteree": 1,
        "ambigu": 1,
        "coupures": 4,
    }
    expect = {
        f"{group}-{i + 1:02}-"
        + (
            "inactif"
            if group == "coupures" or group in ("standard", "grand-dd") and i % 2
            else "actif"
        )
        for group, n in phases.items()
        for i in range(n)
    }
    require(set(obs["cas"]) == expect, "Cellules manquantes")
    ids = set()
    noms_refus = {
        "grand-dd": "GPT secondaire hors fin du support",
        "sans-logs": "Profil GPT/FAT32 ou contenu EFI altéré",
        "fat-alteree": "Profil GPT/FAT32 ou contenu EFI altéré",
        "ambigu": "Identité USB ambiguë",
    }
    for name, d in obs["cas"].items():
        q = d["observation"]
        r = d["rapport"]
        group, seq, mode = name.rsplit("-", 2)
        seq = int(seq)
        require(
            re.fullmatch("[0-9a-f]{16}", r["demarrage"]) and r["demarrage"] not in ids,
            "Identifiant de rapport",
        )
        ids.add(r["demarrage"])
        require(
            q["compteurs_entree"] == q["compteurs_hlt"]
            and not any(q["compteurs_hlt"]["interne"].values())
            and q["trace_commandes_sata_octets"] == 0
            and q["compteurs_entree"]["usb5n"]["rd_bytes"] > 0
            and q["compteurs_entree"]["usb5n"]["wr_bytes"] == 0,
            "Activité disque",
        )
        require(
            q["vm_arretee"]
            and q["interne_intact"]
            and q["lecture_seule"]
            and q["mode_banc"] == mode
            and q["fdset"] == "/dev/fdset/1"
            and "/USB(0x0,0x0)" in q["origine"]
            and q["supports_usb"] == (2 if group == "ambigu" else 1)
            and q["sections_verifiees"] == 5
            and q["pe"] == PE_SHA
            and q["verdict"]["statut"] == 16
            and q["verdict"]["confiance"] == 2
            and r["verdict"] == "refus"
            and r["secure_boot_diagnostic"] == "indéterminé",
            "Attribution EFI",
        )
        refusal = noms_refus.get(group, "")
        if group == "standard" and seq in (9, 10):
            refusal = ("NOVALOGS saturée", "Sceau ou ancien rapport altéré")[seq - 9]
        if group == "coupures":
            refusal = (
                "Interruption simulée après synchronisation",
                "",
                "Coupure simulée pendant le sceau",
                "Sceau ou ancien rapport altéré",
            )[seq - 1]
        require(
            d["refus_export"] == refusal and bool(d["emplacement"]) == (not refusal),
            "Décision d’export",
        )
        if refusal and not (group == "coupures" and seq in (1, 3)):
            require(d["sha_avant"] == d["sha_apres"], "Mutation lors du refus")
    archive = root / "captures/rapports.zip"
    mac = json.loads((root / "lecture-macos.json").read_text())
    win = json.loads((root / "lecture-windows.json").read_text())
    require(
        mac["lecture_seule"]
        and set(mac["fichiers"]) == names
        and win["fichiers_identiques"] == 24
        and empreinte(archive.read_bytes()) == win["archive_sha256"],
        "Lectures externes",
    )
    linux = json.loads((root / "lecture-linux.json").read_text())
    require(
        linux["lecture_seule"] and linux["fichiers"] == mac["fichiers"],
        "Vue vfat Linux",
    )
    with zipfile.ZipFile(archive) as z:
        require(
            set(z.namelist()) == names and len(z.namelist()) == 24,
            "Archive de journaux",
        )
        for i in range(1, 9):
            prefix = f"BOOT{i:04d}/"
            a, b, c = [
                z.read(prefix + n)
                for n in ("rapport.json", "JOURNAL.TXT", "VALIDES.SHA")
            ]
            row = obs["cas"][f"standard-{i:02}-" + ("actif" if i % 2 else "inactif")]
            require(
                json.loads(a) == row["rapport"]
                and row["emplacement"] + "/" == prefix
                and c.decode().splitlines()[:2]
                == [empreinte(a) + "  rapport.json", empreinte(b) + "  JOURNAL.TXT"],
                "Rapport ou sceau",
            )
        require(
            all(mac["fichiers"][n] == empreinte(z.read(n)) for n in names), "Vue macOS"
        )
        # Reconstruire les huit états du support complet à partir des seuls fichiers relus.
        disk = bytearray(raw)
        areas = verify(io.BytesIO(raw))
        for i, slot in enumerate(areas):
            row = obs["cas"][
                f"standard-{i + 1:02}-" + ("actif" if i % 2 == 0 else "inactif")
            ]
            require(empreinte(disk) == row["sha_avant"], "Support avant publication")
            for (pos, length), filename in zip(
                slot, ("rapport.json", "JOURNAL.TXT", "VALIDES.SHA")
            ):
                content = z.read(f"BOOT{i + 1:04d}/" + filename)
                require(len(content) == length, "Écriture hors préallocation")
                disk[pos : pos + length] = content
            require(
                empreinte(disk) == row["sha_apres"], "Octets modifiés hors du rapport"
            )
    modes = json.loads((root / "modes.json").read_text())
    require(
        modes["actif"]["decision"] == "EFI_ACCESS_DENIED"
        and not modes["actif"]["lancement"]
        and modes["inactif"]["pe"] == modes["actif"]["temoin"]
        and modes["inactif"]["verdict"]["statut"] == 16,
        "Contre-épreuve Secure Boot",
    )
    return "GPT, deux FAT32, PE 5M, 20 démarrages mesurés, 8 rapports et 24 fichiers contre-lus"


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("dossier", nargs="?", type=Path, default=PROOF)
    print(verify_delivery(p.parse_args().dossier))
