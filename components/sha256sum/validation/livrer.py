#!/usr/bin/env python3
"""Assembler les exécutables exercés, deux supports de démonstration et les relevés."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

src = Path(__file__).resolve().parents[1]
repo = src.parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("preuves", type=Path)
parser.add_argument("qualification", type=Path, help="campagne finale avec noyaux et ELF séparés")
parser.add_argument("destination", type=Path, help="nouveau répertoire à créer")
args = parser.parse_args()
proof = args.preuves.resolve(strict=True)
qual = args.qualification.resolve(strict=True)
dest = args.destination.resolve()
dest.mkdir(exist_ok=False)


def copy(source, relative):
    target = dest / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)
    return target


def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def notices():
    bare = ["x86_64-unknown-none", "aarch64-unknown-none-softfloat"]
    manifests = [
        (src, ["aarch64-apple-darwin"]),
        (repo / "components", bare),
        (repo / "novaOS/user/novafs", bare),
    ]
    packages = {}
    for folder, platforms in manifests:
        for platform in platforms:
            raw = subprocess.check_output([
                "rustup", "run", "stable", "cargo", "metadata", "--format-version", "1",
                "--locked", "--offline", "--manifest-path", folder / "Cargo.toml",
                "--filter-platform", platform,
            ])
            for item in json.loads(raw)["packages"]:
                packages[item["id"]] = item

    entries = []
    for item in sorted(packages.values(), key=lambda p: (p["name"], p["version"])):
        folder = Path(item["manifest_path"]).parent
        files = [p for p in folder.iterdir() if p.is_file() and
                 p.name.upper().startswith(("LICENSE", "COPYRIGHT", "NOTICE", "COPYING", "UNLICENSE"))]
        if item["name"] == "novaos-storage":
            files = [repo / "novaOS/LICENSE"]
        if not files:
            raise ValueError(f"Licence introuvable pour {item['id']}")
        vcs = folder / ".cargo_vcs_info.json"
        entries.append(dict(nom=item["name"], version=item["version"], licence=item["license"],
                            commit=json.loads(vcs.read_text())["git"]["sha1"] if vcs.exists() else None))
        for f in files:
            copy(f, Path("licences") / (item["name"] + "-" + item["version"]) / f.name)
    (dest / "licences/paquets.json").write_text(json.dumps(entries, indent=2) + "\n")
    copy(repo / "novaOS/LICENSE", "licences/NovaOS/LICENSE")
    for file in (repo / "novaOS/licenses").iterdir():
        if file.is_file():
            copy(file, Path("licences/NovaOS") / file.name)


binary = copy(src / "target/release/sha256sum", "sha256sum")
assert subprocess.check_output([binary, "--version"]) == b"sha256sum (NovaOS) 0.24.0\n"
for name in ["options.py", "performances.py", "tester-options-sha256sum.sh", "mesurer-performances-sha256sum.sh"]:
    copy(src / "validation" / name, name)
for name in ["ETUDE.md", "CONSTRUCTION.md", "RECETTE-NSH.md", "MATRICE.md", "QUALIFICATION.md", "LICENSE"]:
    copy(src / name, name)
copy(src / "sha256sum.1", "man/fr/man1/sha256sum.1")
copy(src / "sha256sum.en.1", "man/man1/sha256sum.1")
notices()

for arch in ["aarch64", "x86_64"]:
    base = qual / arch
    elf = copy(base / "sha256sum-ordinaire.elf", f"novaos/{arch}/sha256sum.elf")
    kernel = base / "noyau-ordinaire.elf"
    assert elf.read_bytes() in kernel.read_bytes()
    vm = json.loads((base / "ordinaire-512/intact/session.vm.json").read_text())
    assert vm["elf_sha256"] == sha(kernel)
    medium = copy(base / "ordinaire-512/intact/session.iso", f"novaos/{arch}/NovaOS-cli.iso")
    assert sha(medium) == vm["iso_sha256"]
    for sector in [512, 4096]:
        for profile, expected in [("ordinaire", 32), ("controle", 33)]:
            run = base / f"{profile}-{sector}"
            output = Path("resultats") / arch / run.name
            assert len(json.loads((run / "observations.json").read_text())) == expected
            copy(run / "observations.json", output.with_suffix(".json"))
            for state in ["intact", "modifie"]:
                for name in ["session.serial", "session.vm.json"]:
                    copy(run / state / name, output / state / name)
            copy(run / "intact/documentation-disque.json", output / "documentation-disque.json")

        run = proof / f"debit-{arch}-{sector}"
        report = json.loads((run / "debits.json").read_text())
        assert len(report["mesures"]) == 17
        assert all(m["octets"] > report["ram_octets"] for m in report["mesures"][-2:])
        for name in ["debits.json", "debit.vm.json", "debit.serial"]:
            copy(run / name, Path("resultats") / arch / f"debit-{sector}" / name)
    for variant in ["memoire", "cpu-memoire", "memoire-grand"]:
        series = json.loads((base / variant / "debits.json").read_text())["mesures"]
        assert len(series) == (17 if variant == "memoire-grand" else 15)
        assert all(m["pic_prive_octets"] for m in series)
        for name in ["debits.json", "debit.vm.json", "debit.serial"]:
            copy(base / variant / name, Path("resultats") / arch / variant / name)

sample = qual / "aarch64/ordinaire-512"
copy(sample / "intact/initiale.raw", "novaos/livraison-P9-512.raw")
copy(sample / "modifie/initiale.raw", "novaos/livraison-modifiee-P9-512.raw")
for name in ["perf-final-novaOS.json", "perf-final-grand-novaOS.json", "perf-macOS.json", "perf-grand-macOS.json",
             "perf-final-portable.json", "moteur-materiel.txt", "moteur-portable.txt"]:
    copy(proof / name, "resultats/darwin/" + name)
assert json.loads((proof / "perf-final-grand-novaOS.json").read_text())["sha256_binaire"] == sha(binary)
regressions = proof / "regressions-04f7af8/statuts.txt"
statuses = [line.split() for line in regressions.read_text().splitlines()]
assert len({(arch, suite) for arch, suite, code in statuses if code == "0"}) == 22
copy(regressions, "resultats/regressions.txt")
for name in ["options-novaOS.txt", "options-macOS.txt", "vecteurs.txt", "formats.txt", "versions.txt", "catalogue-darwin.txt"]:
    copy(proof / name, "resultats/" + name)

files = sorted(p.relative_to(dest).as_posix() for p in dest.rglob("*") if p.is_file())
with (dest / "SHA256SUMS").open("xb") as output:
    subprocess.run([binary, "--", *files], cwd=dest, stdout=output, check=True)
for tool in [binary, "/opt/homebrew/bin/gsha256sum"]:
    subprocess.run([tool, "--quiet", "-c", "SHA256SUMS"], cwd=dest, check=True)
print(len(files), "fichiers relus avec le candidat et GNU.")
