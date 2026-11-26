"""Construction et rejeu de séries Ethernet v1 sur le même banc que la phase 3."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

NOVA = Path(__file__).resolve().parents[2]
ARCH = {"arm": ("aarch64-unknown-none-softfloat", "net-arm-lab"),
        "q35": ("x86_64-unknown-none", "net-q35-lab")}


def compiler(dest, modes):
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=NOVA, text=True).strip()
    if subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=NOVA):
        raise ValueError("Construction sur sources suivies modifiées")
    target = Path(os.environ["NOVAOS_TMPDIR"]) / "eth3b-build"
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
    for mode in modes:
        out = dest / mode
        out.mkdir(parents=True, exist_ok=False)
        sums = {}
        with (out / "compilation.log").open("xb") as log:
            for arch, (triple, feature) in ARCH.items():
                env = dict(os.environ, RUSTC=rustc, NOVAOS_ETH3_CASE=mode, CARGO_TARGET_DIR=str(target))
                subprocess.run(["rustup", "run", "stable", "cargo", "build", "--release", "--locked",
                                "--offline", "--features", feature + ",ethernet-v1-port", "--target", triple],
                               cwd=NOVA, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
                elf = out / (arch + ".elf")
                shutil.copyfile(target / triple / "release/novaos", elf)
                with elf.open("rb") as stream:
                    sums[arch] = hashlib.file_digest(stream, "sha256").hexdigest()
        if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=NOVA,text=True).strip()!=commit or subprocess.check_output(["git", "diff", "HEAD", "--name-only"], cwd=NOVA):
            raise ValueError("Sources modifiées pendant la construction")
        (out / "construction.json").write_text(json.dumps(dict(commit=commit, elf=sums), indent=2)+"\n")


def rejouer(images, sortie, modes, repetitions):
    wt = NOVA.parent.parent
    outils = wt / "archives-ethernet-phase3/reproduction-8bd3f9c"
    for numero in range(1, repetitions + 1):
        for mode in modes:
            src = images / mode
            commit = json.loads((src / "construction.json").read_text())["commit"]
            for arch in ARCH:
                out = sortie / mode / arch / str(numero)
                if out.exists():
                    if json.loads((out/"campagne.json").read_text())["succes"]:
                        continue
                    raise ValueError(f"Essai incomplet à conserver : {out}")
                subprocess.run([sys.executable, "-B", str(Path(__file__).with_name("port_qemu.py")),
                                arch, mode, str(src / (arch + ".elf")), str(out), str(wt / "pilote-virtio-net"),
                                str(wt / "archives-virtio-net"), str(outils / "limine-enroll"),
                                str(outils / "BOOTAA64.EFI"), "--source-elf", commit, "--delai", "150",
                                *(["--limine-arm",str(outils / "limine-a72.EFI")] if arch == "arm" else [])], check=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("action", choices=("compiler", "rejouer"))
    p.add_argument("images", type=Path)
    p.add_argument("modes", nargs="+")
    p.add_argument("--sortie", type=Path)
    p.add_argument("--repetitions", type=int, default=3)
    a = p.parse_args()
    if a.action == "compiler":
        from port_qemu import qualification
        # Les constructions Rust ne doivent pas concurrencer le processus QEMU mesuré.
        with qualification(Path(os.environ["NOVAOS_TMPDIR"]).resolve().parent, {}):
            compiler(a.images.resolve(), a.modes)
    elif a.sortie and a.repetitions >= 1:
        rejouer(a.images.resolve(), a.sortie.resolve(), a.modes, a.repetitions)
    else:
        p.error("--sortie et un nombre positif de démarrages sont requis")
