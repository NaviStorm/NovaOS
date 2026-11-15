"""Exécute un ELF v0.20.0 déjà qualifié sur le noyau courant, sans recompiler l’application."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from threads import BUILD, CARGO, ROOT, TARGETS, exercise, exercise_example


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("architecture", choices=TARGETS)
    parser.add_argument("profil", choices=("sdk-example", "threads-test"))
    parser.add_argument("elf", type=Path)
    args = parser.parse_args()
    arch, profile = args.architecture, args.profil
    reference = json.loads((ROOT / "docs/validation/rust-sdk-phase2-v0.20.0.json").read_text())
    expected = next(r["elf_sha256"] for r in reference["vm"]
                    if (r["architecture"], r["profil"]) == (arch, profile))
    original = args.elf.resolve(strict=True)
    if digest(original) != expected:
        parser.exit(1, "L’ELF fourni diffère de la qualification v0.20.0.\n")
    out = BUILD / "abi-registre" / arch / profile
    out.mkdir(parents=True, exist_ok=True)
    target = TARGETS[arch]
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
    subprocess.run([*CARGO, "build", "-p", "novaos", "--release", "--offline", "--locked",
                    "--target", target, "--features", "thread-probe"], cwd=ROOT, check=True,
                   env={**os.environ, "RUSTC": rustc, "CARGO_TARGET_DIR": str(out),
                        "NOVA_THREAD_PROBE_ELF": str(original)})
    kernel = out / target / "release/novaos"
    copies = list(kernel.parent.glob("build/novaos-*/out/thread-witness.elf"))
    if len(copies) != 1 or digest(copies[0]) != expected:
        parser.exit(1, "L’image copiée dans le catalogue ne correspond pas à l’archive.\n")
    os.environ.update(NOVA_ARCH=arch, NOVA_KERNEL_ELF=str(kernel))
    if profile == "sdk-example":
        result = exercise_example(arch, 2)
        suffix = ""
    else:
        suffix = "-archive"
        result = exercise(arch, 2, suffix)
    serial = BUILD / f"serie-{arch}-{profile}-2cpu{suffix}.log"
    report = {"architecture": arch, "profil": profile, "elf_sha256": expected,
              "noyau_sha256": digest(kernel), "journal": serial.name, "journal_sha256": digest(serial),
              "qemu": subprocess.check_output([f"qemu-system-{arch}", "--version"],
                                              text=True).splitlines()[0], "resultat": result}
    (out / "resultat.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
