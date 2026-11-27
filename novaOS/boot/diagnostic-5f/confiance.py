"""Contre-épreuve Authenticode hors invité sur le PE réellement présent dans chaque FAT."""

import argparse
import json
import os
import re
import subprocess
import tempfile
from datetime import UTC, datetime
from hashlib import sha256
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives.serialization import Encoding
from pyfatfs.PyFatFS import PyFatFS
from virt.firmware.efi.siglist import EfiSigDB
from virt.firmware.varstore.edk2 import Edk2VarStore


def inventaire(store):
    vs = Edk2VarStore(store).get_varlist()
    result = {}
    for key in ("PK", "KEK", "db", "dbx"):
        rows = []
        for group in EfiSigDB(vs[key].data):
            for item in group:
                if group.x509 is not None:
                    rows.append(
                        {
                            "type": "certificat",
                            "sha256": sha256(item["data"]).hexdigest(),
                        }
                    )
                elif (
                    str(group.guid) == "c1c41626-504c-4092-aca9-41f936934328"
                    and len(item["data"]) == 32
                ):
                    rows.append({"type": "empreinte-PE", "sha256": item["data"].hex()})
                else:
                    raise ValueError(
                        "Entrée de confiance inattendue dans les vecteurs 5E"
                    )
        result[key] = rows
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("supports", type=Path)
    parser.add_argument("sortie", type=Path)
    parser.add_argument("--outil", default="osslsigncode")
    args = parser.parse_args()
    src = args.supports.resolve()
    stores = {n: inventaire(src / (n + ".fd")) for n in ("nominal", "revoque")}
    keys = {}
    for key in ("PK", "KEK", "db"):
        cert = x509.load_pem_x509_certificate((src / (key + ".pem")).read_bytes())
        keys[key] = sha256(cert.public_bytes(Encoding.DER)).hexdigest()
        if any(
            v[key] != [{"type": "certificat", "sha256": keys[key]}]
            for v in stores.values()
        ):
            raise ValueError(key)
    if len(set(keys.values())) != 3:
        raise ValueError("Rôles PK, KEK, db confondus")
    cases = {}
    # Date de référence de la série, à l'intérieur de la validité du certificat de test.
    instant = datetime(2026, 11, 27, 20, tzinfo=UTC)
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp:
        pe = Path(tmp) / "BOOTX64.EFI"
        for name in (
            "diagnostic",
            "ancienne",
            "signature-interne",
            "epoque",
            "sans-signature",
            "substitution",
            "configuration",
            "noyau",
        ):
            with PyFatFS(str(src / (name + ".img")), read_only=True) as volume:
                pe.write_bytes(volume.readbytes("/EFI/BOOT/BOOTX64.EFI"))
            check = subprocess.run(
                [
                    args.outil,
                    "verify",
                    "-time",
                    str(int(instant.timestamp())),
                    "-CAfile",
                    str(src / "db.pem"),
                    "-in",
                    str(pe),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            log = check.stdout + check.stderr
            good = check.returncode == 0
            expected = name in ("diagnostic", "ancienne", "signature-interne", "epoque")
            if good != expected or (good and "Signature verification: ok" not in log):
                raise ValueError(name + " " + log)
            if not good and not any(
                x in log.lower() for x in ("no signature found", "mismatch")
            ):
                raise ValueError("Cause de refus non identifiée: " + log)
            calculated = re.search(
                r"Calculated message digest\s*:\s*([0-9A-F]{64})", log
            )
            cases[name] = {
                "sha256_fichier": sha256(pe.read_bytes()).hexdigest(),
                "authenticode": calculated[1].lower() if calculated else None,
                "verifie": good,
                "sortie": log.replace(str(pe), "BOOTX64.EFI").replace(
                    str(src), "supports"
                ),
            }
    previous = {"type": "empreinte-PE", "sha256": cases["ancienne"]["authenticode"]}
    if stores["nominal"]["dbx"] != [previous] or sorted(
        x["sha256"] for x in stores["revoque"]["dbx"]
    ) != sorted((previous["sha256"], keys["db"])):
        raise ValueError("Révocations de la série différentes des témoins attendus")
    result = {
        "date_verification": instant.isoformat(),
        "outil": subprocess.check_output(
            [args.outil, "--version"], text=True
        ).splitlines()[0],
        "magasins": stores,
        "certificats": keys,
        "cas": cases,
    }
    with args.sortie.open("x") as dest:
        dest.write(json.dumps(result, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
