"""Rassembler une campagne REALPATH terminée et relire tous les membres après décompression."""
import argparse
import hashlib
import json
from fractions import Fraction
from pathlib import Path
import tarfile

DELIVERY = Path(__file__).resolve().parents[1]
PROOFS = DELIVERY / "preuves"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verify():
    expected = {}
    for line in (DELIVERY / "qualification.sha256").read_text().splitlines():
        sha, name = line.split("  ", 1)
        assert name not in expected
        expected[name] = sha
    with tarfile.open(DELIVERY / "qualification.tar.xz", "r|xz") as archive:
        for item in archive:
            assert item.isfile() and item.name in expected
            with archive.extractfile(item) as stream:
                assert hashlib.file_digest(stream, "sha256").hexdigest() == expected.pop(item.name)
    assert not expected
    print("Manifeste conforme après décompression.")


def darwin_table():
    rows = ["| Série | Charge (512 chemins) | Publié ms | Candidat ms | GNU ms | Candidat/GNU | MAD candidat ms |",
            "|---:|---|---:|---:|---:|---:|---:|"]
    for series in (1, 2):
        data = json.loads((PROOFS / f"cddb17a/serie-{series}.json").read_text())
        assert data["sha256"]["candidat"] == digest(DELIVERY / "bin/realpath")
        assert data["sha256"]["publie"] == digest(DELIVERY.parent / "livraison-realpath-0240/bin/realpath")
        assert data["sha256"]["gnu"] == "4ca1c1b46a3ef03c29002510570eda7c5bec2a0808a1a3699b37216b4347cee9"
        assert len(data["lots"]) == len({r["charge"] for r in data["lots"]}) == 6
        ratios = []
        for lot in data["lots"]:
            medians = []
            for key in ("publie", "candidat", "gnu"):
                values = sorted(r["ns"] for r in lot["passages"] if r["programme"] == key)
                assert len(values) == 18
                medians.append(Fraction(values[8] + values[9], 2))
            ratio = medians[1] / medians[2]
            assert ratio <= Fraction(11, 10)
            ratios.append(ratio)
            mad = lot["statistiques"]["candidat"]["mad_ns"] / 1e6
            times = " | ".join(f"{float(v)/1e6:.4f}" for v in medians)
            rows.append(f"| {series} | {lot['charge']} | {times} | {float(ratio):.6f} | {mad:.4f} |")
        assert float(max(ratios)) == data["pire_ratio"]
    (PROOFS / "tableau-darwin.md").write_text("\n".join(rows) + "\n")


def collect(run):
    darwin_table()
    members = {}
    table = ["| ISA / secteur P9 | Jeux moteur | Appels ELF | Normalisé, chemins/s | Via lien, chemins/s | Profond, chemins/s | Tas Kio |",
             "|---|---:|---:|---:|---:|---:|---:|"]
    for arch in ("aarch64", "x86_64"):
        for sector in (512, 4096):
            rows = {}
            for kind in ("moteur", "elf"):
                folder = run / f"{kind}-{arch}-{sector}"
                record = json.loads((folder / "resultats.json").read_text())
                rows[kind] = record
                assert record.get("arch", record.get("architecture")) == arch and record["secteur"] == sector
                assert digest(folder / "session.raw") == record.get("disque", record.get("p9"))
                assert digest(folder / "noyau-demarre.elf") == record["noyau"]
                for path in folder.iterdir():
                    if path.is_file() and not path.name.startswith("."):
                        members[f"qemu/{arch}/{sector}/{kind}/{path.name}"] = path
            engine, elf = rows["moteur"], rows["elf"]
            assert len(engine["cas"]) == 57 and len(elf["cas"]) == 8
            assert elf["elf"] == digest(DELIVERY / f"bin/realpath-{arch}.elf")
            rates = " | ".join(f"{engine['medianes'][str(n)]['chemins_s']:.0f}" for n in [1, 32, 4])
            heap = max(r["tas_octets"] for r in engine["mesures"])/1024
            table.append(f"| {arch} / {sector} | 57 | 8 | {rates} | {heap:g} |")
    (PROOFS / "tableau-qemu.md").write_text("\n".join(table) + "\n")
    for folder in ("preuves", "bin", "lab"):
        for p in (DELIVERY/folder).rglob("*"):
            if p.is_file() and not any(c.startswith(".") or c == "__pycache__" for c in p.parts):
                members[str(p.relative_to(DELIVERY))] = p
    members["README.md"] = DELIVERY/"README.md"
    with tarfile.open(DELIVERY / "qualification.tar.xz", "w:xz", preset=3) as archive:
        for name, path in sorted(members.items()):
            archive.add(path, arcname=name, recursive=False)
    (DELIVERY / "qualification.sha256").write_text("".join(
        f"{digest(p)}  {name}\n" for name, p in sorted(members.items())))
    verify()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--recette", type=Path)
    group.add_argument("--verifier", action="store_true")
    args = parser.parse_args()
    if args.verifier:
        verify()
    else:
        collect(args.recette)
