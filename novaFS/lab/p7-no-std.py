#!/usr/bin/env python3
"""Compiler la source cryptographique P7 et ses dépendances sur les deux cibles nues."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = tomllib.loads((ROOT / "Cargo.toml").read_text())
CRATES = ("argon2", "chacha20poly1305", "hkdf", "hmac", "poly1305", "sha2-p7", "zeroize")


def dependency(name):
    fields = MANIFEST["dependencies"][name]
    return name + " = { " + ", ".join(
        key + " = " + json.dumps(value) for key, value in fields.items()
    ) + " }"


with tempfile.TemporaryDirectory(prefix="novafs-p7-no-std-") as tmp:
    work = Path(tmp)
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"],
                                    text=True).strip()
    (work / "Cargo.toml").write_text(
        '[package]\nname = "novafs-crypto-audit"\nversion = "0.0.0"\nedition = "2024"\n'
        '[workspace]\n[lib]\npath = "lib.rs"\n[dependencies]\n'
        + "\n".join(map(dependency, CRATES)) + "\n"
    )
    # Seuls le type d'erreur et son alias viennent du banc. La source testée
    # est celle du projet. L'adaptateur getrandom est exclu sur target_os=none.
    (work / "lib.rs").write_text(
        '#![no_std]\n#[derive(Debug)] pub enum Error { Invalid(&\'static str),'
        " Limit(&'static str), Corrupt(&'static str), Unavailable, Busy, NoMemory }\n"
        "pub type Result<T> = core::result::Result<T, Error>;\n"
        + "#[path=" + json.dumps(str(ROOT / "src/crypto.rs")) + "] pub mod crypto;\n"
    )
    shutil.copyfile(ROOT / "Cargo.lock", work / "Cargo.lock")
    for target in ("aarch64-unknown-none-softfloat", "x86_64-unknown-none"):
        env = dict(os.environ, RUSTC=rustc)
        if target.startswith("aarch64"):
            # Le backend ARMv8 SHA utilise NEON, incompatible avec l'ABI softfloat.
            env["RUSTFLAGS"] = '--cfg sha2_backend="soft"'
        subprocess.run(["rustup", "run", "stable", "cargo", "check", "--offline",
                        "--target", target], cwd=work, env=env, check=True)
