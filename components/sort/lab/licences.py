"""Rassemble les avis des paquets du verrou Cargo dans une archive reproductible."""
import gzip
import io
import json
from pathlib import Path
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]
packages = json.loads(subprocess.check_output(
    ["rustup", "run", "stable", "cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
    cwd=ROOT))["packages"]
used = set()
for target in ("aarch64-apple-darwin", "aarch64-unknown-none-softfloat", "x86_64-unknown-none"):
    tree = subprocess.check_output(["rustup", "run", "stable", "cargo", "tree", "--locked",
        "--offline", "--target", target, "--prefix", "none", "--format", "{p}"], cwd=ROOT, text=True)
    used.update(tuple(line.split()[:2]) for line in tree.splitlines())
entries = {}
catalogue = []
for p in sorted(packages, key=lambda p: p["name"]):
    if not p["source"] or (p["name"], "v"+p["version"]) not in used:
        continue
    folder = Path(p["manifest_path"]).parent
    prefix = f'{p["name"]}-{p["version"]}'
    notices = sorted(f for f in folder.iterdir() if f.is_file()
                     and f.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT")))
    assert notices, prefix
    catalogue.append(dict(paquet=prefix, licence=p["license"], origine=p["repository"],
                          fichiers=[f.name for f in notices]))
    for f in notices:
        entries[f"{prefix}/{f.name}"] = f.read_bytes()
entries["catalogue.json"] = (json.dumps(catalogue, indent=2)+"\n").encode()
with (ROOT/"artifacts"/"licences-cargo.tar.gz").open("wb") as file:
    with gzip.GzipFile(filename="", mode="wb", fileobj=file, mtime=0) as gz:
        with tarfile.open(fileobj=gz,mode="w") as archive:
            for path, data in sorted(entries.items()):
                info = tarfile.TarInfo(path)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
print(f"{len(catalogue)} paquets, {len(entries)-1} notices.")
