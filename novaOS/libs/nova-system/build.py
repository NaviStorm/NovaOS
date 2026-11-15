"""Construit une application native avec le SDK NovaOS extrait."""
import argparse
import os
from pathlib import Path
import subprocess


TARGETS = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}


def main():
    sdk = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=TARGETS)
    parser.add_argument("manifest", nargs="?", type=Path, default=sdk / "examples/threads/Cargo.toml")
    parser.add_argument("--features", help="features Cargo de l’application")
    args = parser.parse_args()
    project = args.manifest.resolve(strict=True)
    flags = ["-Crelocation-model=static", "-Cno-redzone=yes", "-Clink-arg=-zmax-page-size=4096",
             f"-Clink-arg=-T{sdk / 'link/novaos.ld'}"]
    toolchain = os.environ.get("NOVA_RUST_TOOLCHAIN", "stable")
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", toolchain, "rustc"], text=True).strip()
    cmd = ["rustup", "run", toolchain, "cargo", "build", "--locked", "--offline", "--release",
           "--target", TARGETS[args.arch], "--manifest-path", str(project)]
    if args.features:
        cmd.extend(["--features", args.features])
    subprocess.run(cmd, cwd=project.parent, check=True,
                   env={**os.environ, "RUSTC": rustc, "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(flags)})


if __name__ == "__main__":
    main()
