"""Embarque les notices des paquets verrouillés du service utilisateur dans l’ISO."""
import json
from pathlib import Path
import subprocess
import sys


def notices(target, destination):
    root = Path(__file__).resolve().parents[1]
    manifest = root / "user" / "novafs" / "Cargo.toml"
    texts = {}
    components = root.parent / "components"
    packages = {}
    for project in (manifest, components / "Cargo.toml"):
        data = subprocess.check_output([
            "rustup", "run", "stable", "cargo", "metadata", "--format-version", "1",
            "--manifest-path", project, "--filter-platform", target, "--locked", "--offline",
        ], cwd=project.parent)
        for package in json.loads(data)["packages"]:
            packages[package["id"]] = package
    for package in packages.values():
        folder = Path(package["manifest_path"]).parent
        files = sorted(p for p in folder.iterdir() if p.is_file()
                       and p.name.upper().startswith(("LICENSE", "COPYRIGHT", "NOTICE")))
        if package["name"] == "novaos-storage":
            files.append(root / "LICENSE")
        if not files:
            raise ValueError(f"Licence introuvable pour {package['name']}")
        for p in files:
            name = f"{package['name']} {package['version']} — {p.name}"
            texts.setdefault(p.read_bytes(), []).append(name)
    with destination.open("wb") as out:
        out.write("Sources NovaFS MPL-2.0 livrées dans novaFS/ du monorepo NovaOS.\n".encode())
        for content, packages in texts.items():
            out.write(("\n" + "\n".join(packages) + "\n\n").encode())
            out.write(content)


if __name__ == "__main__":
    notices(sys.argv[1], Path(sys.argv[2]))
