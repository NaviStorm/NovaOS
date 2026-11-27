"""Contre-lecture hors QEMU des pièces archivées. Bibliothèque standard Python."""

import hashlib
import json
from pathlib import Path
import tarfile

from domaines import legacy


def need(value, msg):
    if not value:
        raise ValueError(msg)


def main():
    root = Path(__file__).resolve().parents[2] / "docs/validation/diagnostic-uefi-5i"
    for line in (root / "SHA256SUMS").read_text().splitlines():
        digest, filename = line.split(maxsplit=1)
        need(
            hashlib.sha256((root / filename).read_bytes()).hexdigest() == digest,
            filename,
        )
    with tarfile.open(root / "observations.tar.gz") as archive:

        def raw(path):
            file = archive.extractfile(path)
            need(file is not None, path)
            return file.read()

        def data(path):
            return json.loads(raw(path))

        published = json.loads((root / "bilan.json").read_bytes())
        for part in ("terminal", "suspendu", "actif", "performance", "rappels"):
            need(published[part] == data(part + "/bilan.json"), "Bilan publié " + part)
        strict = data("terminal/bilan.json")
        denial = strict["refus_dbx"]
        need(
            denial["rapport"] is None
            and denial["disque_intact"]
            and denial["io_firmware"]["rd_operations"] == 10
            and denial["io_firmware"]["rd_bytes"] == 283136
            and b"No bootable option" in raw("terminal/refus-dbx/firmware.log"),
            "Refus dbx",
        )
        need(
            not strict["serie_sans_ecriture"]
            and sum(v["zero_ecriture"] for v in strict["cas"].values()) == 10,
            "Contre-exemple de la série terminale",
        )
        frozen = data("suspendu/bilan.json")
        need(
            len(strict["cas"]) == 11 and set(strict["cas"]) == set(frozen["cas"]),
            "Cellules",
        )
        for k, sample in frozen["cas"].items():
            base = "suspendu/" + k + "/"
            trace = raw(base + "transactions.log")
            r = data(base + "point-entree.json")
            need(
                sample["disque_intact"]
                and sample["avant"] == sample["apres"]
                and not any(sample["apres"].values())
                and len(trace) == sample["trace_octets"]
                and r["rip"] == sample["rip"] == r["base"] + r["rva"]
                and r["code_entree"].startswith("fa"),
                "Capture suspendue " + k,
            )
            e = strict["cas"][k]
            evidence = "terminal/" + k + "/"
            events = raw(evidence + "transactions.log")
            clean = (
                e["coupe_entree"]
                == e["coupe_scellement"]
                == e["trace_octets"]
                == len(events)
            )
            need(
                clean == e["zero_ecriture"]
                and not any(e["io"].values())
                and not e["rapport"]["vivants"],
                "Verdict EFI " + k,
            )
            need(
                raw(evidence + "modele-entree.json")
                == raw(evidence + "modele-fin.json"),
                "Mutation " + k,
            )
            if k != "invalide":
                inv = data(base + "inventaire.json")
                need(
                    inv["portees"] == data(evidence + "inventaire.json")["portees"],
                    "Inventaires " + k,
                )
            else:
                need(b"DMAR" in raw(base + "refus.txt"), "Refus ACPI")
        previous = raw("contre-exemple-cli/transactions.log")
        need(
            b"memory_region_ops_write" in previous and b"name 'apic-msi'" in previous,
            "Contre-exemple de reprise",
        )
        t = data("rappels/bilan.json")
        need(len(t["cas"]) == 7 and not t["efi_qualifie"], "Témoin callbacks")
        for k, v in t["cas"].items():
            trace = raw("rappels/" + k + "/transactions.log")
            need(
                not v["zero_ecriture_efi"]
                and b"memory_region_ops_write" in trace[v["coupe_entree"] :],
                "Témoin " + k,
            )
        a = data("actif/bilan.json")
        need(a["trace_avant"] == a["trace_apres"], "Mutation pendant la capture VT-d")
        first = {}
        for address, expected in a["pages"].items():
            name = f"{int(address, 16):x}.bin"
            buf = raw("actif/avant/" + name)
            need(
                buf == raw("actif/apres/" + name)
                and hashlib.sha256(buf).hexdigest() == expected,
                "Mutation de table",
            )
            first[int(address, 16)] = buf
        decoded = legacy(
            next(iter(a["registres"]["unites"].values())),
            first,
            set(a["donnees_autorisees"]),
        )
        need(
            {str(k): v for k, v in decoded.items()} == a["domaines"]
            and not any(a["io"]["stockage"].values()),
            "Domaines actifs",
        )
        need(len(data("performance/bilan.json")["paires"]) == 12, "Mesures appariées")
    print(
        "11 captures suspendues, verdicts EFI, refus dbx, contre-exemples, 10 pages de tables et 12 mesures relus."
    )


if __name__ == "__main__":
    main()
