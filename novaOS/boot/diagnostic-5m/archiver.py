"""Liste fermée des pièces hors ligne de la phase 5M. Ne prend aucun répertoire de clés."""

import argparse
from hashlib import sha256
from pathlib import Path
import sys
import json
from pyfatfs.PyFatFS import PyFatFS

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5l"))
from pieces import archive


def main():
    p = argparse.ArgumentParser()
    for n in ("supports", "serie", "sortie"):
        p.add_argument(n, type=Path)
    a = p.parse_args()
    a.sortie.mkdir(parents=True, exist_ok=False)

    def supports():
        for nom in (
            "valide.efi",
            "manifeste.efi",
            "paquet.efi",
            "actif.fd",
            "inactif.fd",
            "images.json",
            "magasin.json",
        ):
            yield nom, (a.supports / nom).read_bytes()
        for nom in ("valide", "manifeste", "paquet"):
            yield nom + ".img", (a.supports / (nom + ".img")).read_bytes()
        yield "hors-db.img", (a.serie / "actif-refus-db/hors-db.img").read_bytes()
        yield "hors-db.efi", (a.serie / "actif-refus-db/hors-db.efi").read_bytes()

    def observations():
        for mode in ("actif", "inactif"):
            yield (
                "cal-" + mode + ".json",
                (a.serie / ("cal-" + mode) / "calibration.json").read_bytes(),
            )
            for cas in (
                "valide",
                "altere",
                "signataire",
                "reserve",
                "absent",
                "perime",
                "reutilise",
                "substitution",
                "manifeste",
                "paquet",
            ) + (("refus-db",) if mode == "actif" else ()):
                dossier = a.serie / (mode + "-" + cas)
                noms = [
                    "resultat.json",
                    "transfert.bin",
                    "epoch.bin",
                    "trace.log",
                    "efi.log",
                    "compteurs-qmp.jsonl",
                    "tables-avant.bin",
                    "tables-apres.bin",
                ]
                if cas not in ("refus-db", "reutilise", "substitution"):
                    noms += [
                        "sortie.bin",
                        "intervalle.log",
                        "interruptions.json",
                        "verdict-apic.json",
                    ]
                fat = dossier / "support.img"
                with PyFatFS(str(fat), read_only=True) as fs:
                    b = fs.readbytes("EFI/BOOT/BOOTX64.EFI")
                raw = fat.read_bytes()
                if (
                    raw.count(b) != 1
                    or raw[-176:] != (dossier / "transfert.bin").read_bytes()
                ):
                    raise ValueError("Support capturé ambigu")
                media = dict(
                    sha256=sha256(raw).hexdigest(),
                    efi=sha256(b).hexdigest(),
                    position=raw.index(b),
                    taille=len(b),
                )
                yield (
                    dossier.name + "/media.json",
                    (json.dumps(media, indent=2) + "\n").encode(),
                )
                for n in noms:
                    yield dossier.name + "/" + n, (dossier / n).read_bytes()

    archive(a.sortie / "supports.tar.gz", supports())
    archive(a.sortie / "observations.tar.gz", observations())
    (a.sortie / "SHA256SUMS").write_text(
        "".join(
            sha256(f.read_bytes()).hexdigest() + "  " + f.name + "\n"
            for f in sorted(a.sortie.glob("*.tar.gz"))
        )
    )


if __name__ == "__main__":
    main()
