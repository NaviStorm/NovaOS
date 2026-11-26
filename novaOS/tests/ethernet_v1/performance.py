"""Construction et rejeu de séries Ethernet v1 sur le même banc que la phase 3."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile

NOVA = Path(__file__).resolve().parents[2]
ARCH = {"arm": ("aarch64-unknown-none-softfloat", "net-arm-lab"),
        "q35": ("x86_64-unknown-none", "net-q35-lab")}


def compiler(dest, modes, revision, detail):
    from port_construire_q35 import git
    commit = git("rev-parse", "--verify", revision + "^{commit}").decode().strip()
    instrumentation = git("rev-parse", "HEAD").decode().strip()
    tmp = Path(os.environ["NOVAOS_TMPDIR"]).resolve(strict=True)
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
    with tempfile.TemporaryDirectory(prefix="eth3b-source-", dir=tmp) as td:
        root = Path(td)
        with subprocess.Popen(["git", "-C", NOVA.parent, "archive", commit, "novaOS", "novaFS", "components"],
                              stdout=subprocess.PIPE) as proc:
            with tarfile.open(fileobj=proc.stdout, mode="r|") as tar:
                tar.extractall(path=root, filter="data")
            if proc.wait() != 0:
                raise ValueError("Extraction de la révision")
        sources = {}
        if detail and commit != instrumentation:
            # Fichiers noyau exclusivement observateurs. Le service, l'adaptateur
            # et le client exécutés sont ceux de la référence instrumentée.
            for name in ("build.rs", "src/sched/process/syscall.rs", "src/sched/process/ipc/bulk.rs",
                         "src/sched/process/ipc/traffic.rs", "src/sched/process/net_preparation/profil_3b.rs",
                         "src/sched/process/net_preparation/profil_detail.rs",
                         "src/sched/process/net_preparation/profil_notifications.rs"):
                rel = Path("novaOS/kernel") / name
                (root/rel).write_bytes(git("show", f"{instrumentation}:{rel}"))
                sources[str(rel)] = git("hash-object", root/rel).decode().strip()
        target = root/"target"
        for mode in modes:
            out = dest / mode
            out.mkdir(parents=True, exist_ok=False)
            elf_hash = {}
            with (out/"compilation.log").open("xb") as log:
                for arch,(triple,feature) in ARCH.items():
                    env = dict(os.environ, RUSTC=rustc, NOVAOS_ETH3_CASE=mode, CARGO_TARGET_DIR=str(target))
                    env.pop("NOVAOS_ETH3B_DETAIL",None)
                    if detail:
                        env["NOVAOS_ETH3B_DETAIL"]="1"
                    subprocess.run(["rustup", "run", "stable", "cargo", "build", "--release", "--locked",
                                    "--offline", "--features", feature + ",ethernet-v1-port", "--target", triple],
                                   cwd=root/"novaOS",env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
                    elf=out/(arch+".elf")
                    shutil.copyfile(target/triple/"release/novaos",elf)
                    with elf.open("rb") as stream:
                        elf_hash[arch]=hashlib.file_digest(stream,"sha256").hexdigest()
            manifest = dict(commit=commit, elf=elf_hash, diagnostic=detail, instrumentation=instrumentation if detail else None,
                            modules_superposes=sources,rustc=subprocess.check_output([rustc,"-V"],text=True).strip())
            (out/"construction.json").write_text(json.dumps(manifest,indent=2)+"\n")


def rejouer(images, sortie, modes, repetitions):
    worktrees = NOVA.parent.parent
    archive = worktrees / "archives-virtio-net"
    outils = worktrees / "archives-ethernet-phase3/reproduction-8bd3f9c"
    for numero in range(1, repetitions + 1):
        for mode in modes:
            variante = "veille" if mode == "filtre" else mode
            source = images / variante
            historique = mode in ("historique100k", "historique1514")
            manifeste = None if historique else json.loads((source / "construction.json").read_text())
            for arch in ARCH:
                if mode == "historique100k":
                    elf = archive / "cloture-20261126" / f"applicabilite-{arch}-endurance.elf"
                elif mode == "historique1514":
                    elf = archive / "finale-20261125" / f"{arch}-l2-12.elf"
                else:
                    elf = source / (arch + ".elf")

                out = sortie / mode / arch / str(numero)
                if out.exists():
                    precedent = json.loads((out / "campagne.json").read_text())
                    with elf.open("rb") as stream:
                        empreinte = hashlib.file_digest(stream, "sha256").hexdigest()
                    if (precedent["succes"] and precedent["cas"] == mode and precedent["arch"] == arch
                            and precedent["elf"] == empreinte):
                        continue
                    raise ValueError(f"Résultat antérieur incomplet ou d'une autre image : {out}")

                commande = [
                    sys.executable, "-B", Path(__file__).with_name("port_qemu.py"),
                    arch, mode, elf, out, worktrees / "pilote-virtio-net", archive,
                    outils / "limine-enroll", outils / "BOOTAA64.EFI",
                    "--delai", "420" if mode == "historique1514" else "220",
                ]
                if manifeste is not None:
                    commande.extend(["--source-elf", manifeste["commit"]])
                if arch == "arm":
                    commande.extend(["--limine-arm", outils / "limine-a72.EFI"])
                subprocess.run(commande, check=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("action", choices=("compiler", "rejouer"))
    p.add_argument("images", type=Path)
    p.add_argument("modes", nargs="+")
    p.add_argument("--sortie", type=Path)
    p.add_argument("--revision", default="HEAD")
    p.add_argument("--detail", action="store_true", help="Chronométrage séparé, hors séries de débit")
    p.add_argument("--repetitions", type=int, default=3)
    a = p.parse_args()
    if a.action == "compiler":
        from port_qemu import qualification
        # Les constructions Rust ne doivent pas concurrencer le processus QEMU mesuré.
        with qualification(Path(os.environ["NOVAOS_TMPDIR"]).resolve().parent, {}):
            compiler(a.images.resolve(), a.modes, a.revision, a.detail)
    elif a.sortie and a.repetitions >= 1:
        rejouer(a.images.resolve(), a.sortie.resolve(), a.modes, a.repetitions)
    else:
        p.error("--sortie et un nombre positif de démarrages sont requis")
