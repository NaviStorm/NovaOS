"""Inventaire des sources verrouillées et notices des distributions Rust."""
import gzip
import json
from pathlib import Path
import subprocess as sp

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / "livraison-date-0240/licences"
DEST.mkdir(exist_ok=True)
targets = ("aarch64-apple-darwin", "aarch64-unknown-none-softfloat", "x86_64-unknown-none")
packages = {}
for target in targets:
    metadata = json.loads(sp.check_output([
        "rustup", "run", "stable", "cargo", "metadata", "--locked", "--offline",
        "--format-version", "1", "--filter-platform", target,
        "--manifest-path", str(ROOT / "components/coreutils/date/Cargo.toml")]))
    for package in metadata["packages"]:
        packages[package["id"]] = package
texts = {}
with (DEST / "dependances.tsv").open("w") as table:
    print("paquet\tversion\tlicence déclarée\tdépôt", file=table)
    for pkg in sorted(packages.values(), key=lambda p: p["name"]):
        folder = Path(pkg["manifest_path"]).parent
        files = [p for p in sorted(folder.iterdir()) if p.is_file()
                 and p.name.upper().startswith(("LICENSE", "LICENCE", "NOTICE", "COPYING", "COPYRIGHT", "UNLICENSE"))]
        if not files and pkg["name"].startswith("nova-"):
            files = [ROOT / "components/LICENSE"]
        assert files, pkg["name"]
        print(pkg["name"], pkg["version"], pkg["license"], pkg["repository"] or "monorepo", sep="\t", file=table)
        for path in files:
            texts.setdefault(path.read_bytes(), []).append(f'{pkg["name"]} {pkg["version"]} / {path.name}')
with (DEST / "NOTICES.txt").open("wb") as output:
    for index, (content, titles) in enumerate(texts.items()):
        if index: output.write(b"\n")
        output.write(("\n".join(titles) + "\n\n").encode() + content.rstrip(b"\n") + b"\n")
sysroot = Path(sp.check_output(["rustup","run","stable","rustc","--print","sysroot"],text=True).strip())
(DEST/"Rust-COPYRIGHT-library.html.gz").write_bytes(gzip.compress((sysroot/"share/doc/rust/COPYRIGHT-library.html").read_bytes(),mtime=0))
