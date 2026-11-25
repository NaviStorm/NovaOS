"""Construire une copie privée du modèle QEMU avec le seul diff RX approuvé."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "docs/validation/virtio-net/ethernet-20261125"
VERSIONS = {
    "10.0.12": ("aarch64", "96c889f9d9f1cdd2413f033378078d95acb3f05b23da2dea31ef9ffd7f6ffc09"),
    "10.0.12-utm": ("aarch64", "7c9605290b34152debb842e965a55d2e4fbba4793e536ab677b4027e5dc0ba1a"),
    "11.1.3": ("x86_64", "5a8289c7940dc77d3873c81a6c29ba3afe1446fc9d36e77fa1658cdbecb84b22"),
}


def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def construire(root, version, python, jobs, essai, dtc):
    arch, expected = VERSIONS[version]
    (root / "compilation").mkdir(exist_ok=True)
    (root / "modeles").mkdir(exist_ok=True)
    archive = root / "amont" / f"qemu-{version}.tar.xz"
    patch = BASELINE / "proposition-num-buffers.diff"
    if sha(archive) != expected or sha(patch) != "64bba00421bea605521de0514a5639730fa61fee2e1de2ee55480608bec832ef":
        raise ValueError("Source ou diff différent de la référence de revue")
    work = Path(os.environ["NOVAOS_TMPDIR"]) / f"net-modele-rx-{version}-{essai}"
    work.mkdir()
    subprocess.run(["tar", "-xf", archive, "-C", work], check=True)
    src = work / f"qemu-{version}"
    out = root / "compilation" / f"{version}-{essai}"
    out.mkdir()
    source = src / "hw/net/virtio-net.c"
    sources = json.loads((BASELINE / "sources-qemu.json").read_text())
    tag = version.removesuffix("-utm")
    prefix = f"https://raw.githubusercontent.com/qemu/qemu/v{tag}/"
    for url, h in sources.items():
        if url.startswith(prefix) and sha(src / url[len(prefix):]) != h:
            raise ValueError("Différence avec le tag épinglé : " + url)
    shutil.copyfile(source, out / "virtio-net-original.c")
    with patch.open("rb") as f:
        subprocess.run(["patch", "-p1", "-F", "0"], cwd=src, stdin=f, check=True)
    shutil.copyfile(source, out / "virtio-net-corrige.c")
    build = work / "build-prive"
    build.mkdir()
    opts = [str(src / "configure"), f"--target-list={arch}-softmmu",
            f"--python={python}", "--disable-docs", "--disable-werror", "--disable-tools",
            "--disable-guest-agent", "--disable-download", "--disable-debug-info",
            "--disable-gtk", "--disable-sdl", "--disable-cocoa", "--disable-vnc",
            "--enable-fdt=system", f"--extra-cflags=-I{dtc}/include", f"--extra-ldflags=-L{dtc}/lib",
            "--disable-hvf", "--enable-tcg", "--enable-trace-backends=log", "--enable-tpm"]
    if tag == "10.0.12":
        opts.append("--disable-pvg")  # Banc sans carte graphique Apple.
    if version.endswith("-utm"):
        opts.append("--disable-dbus-display")  # Console série, aucun affichage D-Bus.
    ninja = ["ninja", f"-j{jobs}", "qemu-system-" + arch]
    info = dict(version=tag, distribution=version, cible=arch, archive=expected, original=sha(out / "virtio-net-original.c"), corrige=sha(source),
                diff=sha(patch), configure=opts, ninja=ninja)
    dest = root / "modeles" / ("qemu-system-" + arch)
    if version.endswith("-utm"):
        dest = dest.parent / "utm" / dest.name
        dest.parent.mkdir(exist_ok=True)
    if dest.exists():
        raise FileExistsError(dest)
    start = time.monotonic()
    try:
        for label, cmd in [("configure", opts), ("ninja", ninja)]:
            with (out / (label + ".log")).open("wb") as f:
                subprocess.run(cmd, cwd=build, stdout=f, stderr=subprocess.STDOUT, check=True)
        shutil.copy2(build / dest.name, dest)
        info["executable"] = str(dest)
        info["sha256"] = sha(dest)
        info["version_executee"] = subprocess.check_output([dest, "--version"], text=True).strip()
    finally:
        info["secondes"] = time.monotonic() - start
        (out / "construction.json").write_text(json.dumps(info, indent=2) + "\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=Path)
    p.add_argument("version", choices=VERSIONS)
    p.add_argument("--python", required=True)
    p.add_argument("--jobs", default=4, type=int, choices=range(1, 17))
    p.add_argument("--dtc", required=True, type=Path)
    p.add_argument("--essai", required=True, type=int, choices=range(1, 100))
    a = p.parse_args()
    construire(a.archive.resolve(), a.version, a.python, a.jobs, a.essai, a.dtc.resolve(strict=True))
