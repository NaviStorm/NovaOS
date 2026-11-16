"""Inventaire du verrou Cargo, des licences présentes et des deux moteurs C de Darwin."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

out = Path(sys.argv[1]).resolve()
out.mkdir(parents=True, exist_ok=True)
packages = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version=1"]))["packages"]
tree = subprocess.check_output(["cargo", "tree", "--locked", "--target", "aarch64-apple-darwin",
                               "-e", "normal,build", "--prefix", "none", "--format", "{p}"]).decode().splitlines()
lock = {(p["name"], p["version"]): p.get("checksum","local") for p in tomllib.loads(Path("Cargo.lock").read_text())["package"]}
rows = ["# Dépendances de l’étape macOS", "",
        "Le verrou comprend les dépendances conditionnelles des autres plateformes. La colonne Darwin comprend aussi les outils de construction. Les alternatives de licence restent visibles. Pour self_cell, l’option retenue est Apache-2.0.", "",
        "| Paquet | Version | Licence déclarée | Darwin ARM64 | SHA-256 de la caisse |",
        "|---|---|---|---|---|"]
for pkg in sorted(packages, key=lambda p:(p["name"],p["version"])):
    name, version = pkg["name"],pkg["version"]
    root = Path(pkg["manifest_path"]).parent
    included = any(line.split()[:2] == [name,"v"+version] for line in tree)
    licence = pkg["license"]
    assert licence and (name,version) in lock
    rows.append(f"| {name} | {version} | {licence} | {'oui' if included else 'non'} | {lock[name,version]} |")
    if not included: continue
    dest = out/(name+"-"+version)
    dest.mkdir(exist_ok=True)
    for f in root.iterdir():
        if f.is_file() and f.name.lower().startswith(("license","licence","copying","notice","unlicense","copyright")):
            shutil.copyfile(f,dest/f.name)
    if name in ("fluent-langneg","intl_pluralrules"):
        # Ces archives déclarent Apache-2.0/MIT dans Cargo.toml.
        shutil.copyfile(root/"Cargo.toml.orig",dest/"Cargo.toml.orig")
        shutil.copyfile("/opt/homebrew/opt/rust/LICENSE-APACHE",dest/"LICENSE-APACHE")
    if name == "onig_sys":
        shutil.copyfile(root/"oniguruma/COPYING",dest/"COPYING-Oniguruma")
    if name == "grep-engine":
        shutil.copyfile("LICENSE",dest/"LICENSE")
    assert any(dest.iterdir()), (name,licence)
assert len(packages) == len(lock)
for src in ("COPYRIGHT","LICENSE-MIT","LICENSE-APACHE","share/doc/rustc/COPYRIGHT-library.html"):
    shutil.copyfile(Path("/opt/homebrew/opt/rust")/src,out/("Rust-"+Path(src).name))
pcre = Path(subprocess.check_output(["pkg-config","--variable=prefix","libpcre2-8"]).decode().strip())
shutil.copyfile(pcre/"LICENCE.md",out/"PCRE2-LICENCE.md")
rows += ["", "Oniguruma C 6.9.10 est embarqué dans onig_sys 69.9.3. Sa licence BSD-2-Clause accompagne celles des liaisons Rust MIT. PCRE2 10.49 utilise BSD-3-Clause WITH PCRE2-exception et son JIT BSD-2-Clause. Le binaire Darwin lie les archives C statiquement. Les deux bibliothèques dynamiques observées sont libSystem et libiconv du système Apple. Les notices de Rust et de sa bibliothèque standard figurent dans l’archive de qualification.", ""]
Path("validation/licences.md").write_text("\n".join(rows))
