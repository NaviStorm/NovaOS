"""Conserve exclusivement les pièces publiques des VM jetables et leurs résultats."""

import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile

EDU = ("nominal", "inactif", "nominal-apic", "inactif-apic", "iqe")
DUO = (
    "duo-retrait",
    "duo-differe",
    "duo-sans-reset",
    "duo-wait-absent",
    "duo-wait-tardif",
)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for arg in ("edu", "duo", "sortie"):
        p.add_argument(arg, type=Path)
    a = p.parse_args()
    a.sortie.mkdir(parents=True, exist_ok=False)
    with (
        (a.sortie / "observations.tar.gz").open("xb") as fichier,
        gzip.GzipFile(filename="", fileobj=fichier, mode="wb", mtime=0) as gz,
        tarfile.open(fileobj=gz, mode="w|") as tar,
    ):

        def ajouter(nom, data):
            info = tarfile.TarInfo(nom)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))

        for cas in EDU:
            noms = [
                "avant.bin",
                "apres.bin",
                "queue.bin",
                "wait.bin",
                "epoch.bin",
                "registres.json",
                "trace.log",
                "compteurs-qmp.jsonl",
                "recu-efi.bin",
                "efi.log",
                "resultat.json",
            ]
            if cas.endswith("-apic"):
                noms.extend(
                    ("intervalle.log", "interruptions.json", "verdict-apic.json")
                )
            else:
                noms.append("calibration.json")
            for f in noms:
                ajouter(cas + "/" + f, (a.edu / cas / f).read_bytes())
        for cas in DUO:
            for f in ("serie.log", "qemu.trace", "namespace.raw", "pair.pcap.gz"):
                ajouter(cas + "/" + f, (a.duo / cas / f).read_bytes())
            d = json.loads((a.duo / cas / "campagne.json").read_text())
            champs = (
                "cas",
                "audit",
                "elf",
                "modele",
                "graine",
                "lecteur",
                "commit_sources",
                "revision_banc",
                "succes",
                "pci_initial",
                "pci_final",
                "qi_finale",
                "qi_incertaine",
                "nvme_actif",
                "tables_finales",
                "trames_par_sens",
                "stockage",
                "p9_final",
            )
            garder = {k: d[k] for k in champs if k in d}
            ajouter(
                cas + "/campagne.json", (json.dumps(garder, indent=2) + "\n").encode()
            )
    archive = a.sortie / "observations.tar.gz"
    (a.sortie / "SHA256SUMS").write_text(
        hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n"
    )


if __name__ == "__main__":
    main()
