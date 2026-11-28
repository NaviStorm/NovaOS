"""Compiler le raccord seulement dans une copie privée des sources des paquets."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
    root = Path.cwd()
    target = Path(sys.argv[1]).resolve()
    target.mkdir(parents=True, exist_ok=True)
    d = root / "novaOS/boot/diagnostic-5o"
    paths = (
        "novaOS/boot/diagnostic-5e",
        "novaOS/boot/diagnostic-5m",
        "novaOS/boot/diagnostic-5j",
    )
    with tempfile.TemporaryDirectory(
        prefix="sources-5o-", dir=os.environ["NOVAOS_TMPDIR"]
    ) as temp:
        work = Path(temp).resolve()
        for s in subprocess.check_output(["git", "ls-files", "-z", "--", *paths]).split(
            b"\0"
        ):
            if s:
                p = Path(os.fsdecode(s))
                t = work / p
                t.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(root / p, t)
        # Cargo consulte les dépendances communes du dépôt. Toutes les sorties vont dans target.
        for path in ("novaOS/libs", "novaOS/user", "novaFS"):
            (work / path).symlink_to(root / path, target_is_directory=True)
        g = work / "novaOS/boot/diagnostic-5o/ecran.rs"
        g.parent.mkdir()
        shutil.copyfile(d / "ecran.rs", g)
        # Les deux objets d'origine protègent également un refus de raccord sur une base divergente.
        for path, oid in (
            (
                "novaOS/boot/diagnostic-5e/Cargo.toml",
                "e080cd1162eae837d9c76264164cb2a8d66c862f",
            ),
            (
                "novaOS/boot/diagnostic-5m/entree.rs",
                "65627e4802eb9e751895ecaff6ce1b3ce6467345",
            ),
        ):
            if (
                subprocess.check_output(["git", "hash-object", work / path])
                .decode()
                .strip()
                != oid
            ):
                raise ValueError("Révision de base différente pour " + path)
        patch = d / "raccord.diff"
        for cmd in (["git", "apply", "--check", patch], ["git", "apply", patch]):
            subprocess.run(cmd, cwd=work, check=True)
        # Garder le même identifiant Cargo pour la bibliothèque de chemin partagée.
        # Un chemin traversant le nom aléatoire de la copie changerait l'ordre de liaison.
        manifest = work / "novaOS/boot/diagnostic-5e/Cargo.toml"
        dep = '"../../libs/catalogue-pilotes"'
        content = manifest.read_text()
        if content.count(dep) != 1:
            raise ValueError("Dépendance catalogue ambiguë")
        manifest.write_text(
            content.replace(
                dep, json.dumps(str(root / "novaOS/libs/catalogue-pilotes"))
            )
        )
        env = dict(os.environ)
        subprocess.run(
            [
                "rustup",
                "run",
                "stable",
                "cargo",
                "build",
                "--bin",
                "diagnostic-5m",
                "--features",
                "diagnostic-5o",
                "--release",
                "--locked",
                "--offline",
                "--target",
                "x86_64-unknown-uefi",
                "--manifest-path",
                "novaOS/boot/diagnostic-5e/Cargo.toml",
                "--target-dir",
                target,
            ],
            cwd=work,
            env=env,
            check=True,
        )

        pe = target / "x86_64-unknown-uefi/release/diagnostic-5m.efi"
        expected = (
            (root / "novaOS/docs/validation/diagnostic-uefi-5o/efi.sha256")
            .read_text()
            .split()[0]
        )
        if hashlib.sha256(pe.read_bytes()).hexdigest() != expected:
            raise ValueError("PE reconstruit différent de l’image qualifiée")


if __name__ == "__main__":
    main()
