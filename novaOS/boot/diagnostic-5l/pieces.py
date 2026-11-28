"""Assemble la liste fermée des observations synthétiques de 5L dans un nouveau dossier."""

import argparse
import gzip
from hashlib import sha256
import io
import json
from pathlib import Path
import tarfile
from verifier import DISQUES, EFI, NOVA


def archive(path, entries):
    with (
        path.open("xb") as out,
        gzip.GzipFile(filename="", fileobj=out, mode="wb", mtime=0) as gz,
        tarfile.open(fileobj=gz, mode="w|") as t,
    ):
        for name, data in entries:
            m = tarfile.TarInfo(name)
            m.size = len(data)
            t.addfile(m, io.BytesIO(data))


def preuves(efi, stockage, pilote):
    for nom in EFI:
        base = efi / nom
        names = (
            "trace.log",
            "intervalle.log",
            "observation.json",
            "apic.json",
            "resultat.json",
            "epoch.bin",
            "recu.bin",
            "compteurs-qmp.jsonl",
            "tables-avant.bin",
            "tables-apres.bin",
            "pci-avant.json",
            "pci-apres.json",
        )
        for name in names + tuple(
            f"{i}-{s}.sha-pages" for i in range(1, 4) for s in ("commande", "retour")
        ):
            yield nom + "/" + name, (base / name).read_bytes()
    for name in ("trace.log", "intervalle.log", "apic.json", "observation.json"):
        yield "concurrence/" + name, (pilote / name).read_bytes()
    yield "concurrence/selection.json", (pilote.parent / "selection.json").read_bytes()
    for nom in (*DISQUES, "temoin"):
        base = stockage / nom
        names = [
            "qemu.trace",
            "serie.log",
            "namespace.raw",
            "pair.pcap.gz",
            "sources.json",
        ]
        if nom.startswith("actif-"):
            names.append("tables-qi.bin")
        for name in names:
            yield nom + "/" + name, (base / name).read_bytes()
        d = json.loads((base / "campagne.json").read_text())
        keys = (
            "cas",
            "succes",
            "audit",
            "modele",
            "elf",
            "graine",
            "lecteur",
            "commit_sources",
            "revision_banc",
            "p9_final",
            "qi_incertaine",
            "stockage",
            "pci_initial",
            "pci_final",
            "domaine_5l",
            "trames_par_sens",
            "faute_qi",
        )
        yield (
            nom + "/campagne.json",
            (json.dumps({k: d[k] for k in keys if k in d}, indent=2) + "\n").encode(),
        )


def main():
    p = argparse.ArgumentParser()
    for n in ("efi", "stockage", "concurrence", "regression", "build5j", "sortie"):
        p.add_argument(n, type=Path)
    a = p.parse_args()
    a.sortie.mkdir(parents=True, exist_ok=False)
    archive(a.sortie / "observations.tar.gz", preuves(a.efi, a.stockage, a.concurrence))
    (a.sortie / "regression-5k.tar.gz").write_bytes(
        (a.regression / "observations.tar.gz").read_bytes()
    )
    with tarfile.open(
        NOVA / "docs/validation/diagnostic-uefi-5j/laboratoire.tar.gz"
    ) as t:
        pieces = [("LICENCES-OVMF.txt", t.extractfile("LICENCES-OVMF.txt").read())]
    for name in (
        "supports5j-b/paquet.efi",
        *(
            "ovmf5j-b/Build/OvmfX64/RELEASE_CLANGDWARF/X64/" + m + ".efi"
            for m in ("SmmControl2Dxe", "VariableSmmRuntimeDxe")
        ),
    ):
        pieces.append((name, (a.build5j / name).read_bytes()))
    archive(a.sortie / "modules.tar.gz", pieces)
    sums = "".join(
        sha256(f.read_bytes()).hexdigest() + "  " + f.name + "\n"
        for f in sorted(a.sortie.glob("*.tar.gz"))
    )
    (a.sortie / "SHA256SUMS").write_text(sums)


if __name__ == "__main__":
    main()
