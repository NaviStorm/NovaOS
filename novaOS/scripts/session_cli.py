"""Préparer un volume de démonstration ou démarrer le shell série sur une image."""

import argparse
import os
from pathlib import Path
import subprocess
import tempfile
import sys

from bloc_qemu import ROOT, command

HOST = ROOT.parent / "novaFS/target/aarch64-apple-darwin/release/novafs"
EXAMPLE = b"NovaOS\n"
# Le second accent traverse la limite de la lecture de 704 octets de cat et head.
LONG = ("début " + "a" * 696 + "é\n").encode() + b"".join(
    f"ligne {i}\n".encode() for i in range(1, 16)
) + b"fin"
BINARY = b"avant\x1b[31m\x00apres"


def fixture(path, sector):
    commands = ["mkdir /documents"]
    for name, data in (
        ("/documents/exemple.txt", EXAMPLE), ("/lignes", LONG), ("/controle", BINARY)
    ):
        commands.extend([f"create {name}", f"write {name} 0 {data.hex()}"])
    commands.extend("mkdir /" + "/".join(["p"] * depth) for depth in range(1, 33))

    # La publication exclusive d'une image complète protège deux lancements concurrents.
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".nova-cli-") as image:
        subprocess.run(
            [HOST, "script", "512", str(sector)],
            check=True,
            input=("\n".join(commands) + "\n").encode(),
            stdout=image,
        )
        image.flush()
        os.link(image.name, path)


def fixture_p9(path, sector):
    """Publier une image NEUVE. Le démarrage sur une image existante ne la convertit jamais."""
    import re
    with tempfile.TemporaryDirectory(dir=path.parent, prefix=".nova-preparation-") as tmp:
        source = Path(tmp) / "fixture.raw"
        fixture(source, sector)
        with source.open("rb") as disk:
            records = subprocess.check_output([HOST, "inspect", "512", str(sector)], stdin=disk)
        ids = re.search(rb"FS=(\w+) LV=(\w+)", records)
        if ids is None:
            raise ValueError("identifiants absents de l’image de préparation")
        subprocess.run([
            sys.executable, ROOT.parent / "novaFS/lab/migrate-p9.py", source, path,
            "--outil", HOST, "--profil", "contenu", "--secteur", str(sector),
            "--fs", ids[1].decode(), "--lv", ids[2].decode(),
        ], check=True)


def machine(arch, sector, image, test=False, manual=False, *, iso=None, historique_p8=False):
    profile = "cli-p8" if historique_p8 else "cli"
    if test:
        profile += "-test"
    if iso is None:
        iso = ROOT / "build" / f"{profile}-{arch}.iso"
    env = {**os.environ, "NOVA_ARCH": arch, "NOVA_ISO": str(iso)}
    subprocess.run(
        ["sh", ROOT / "scripts/iso.sh", profile, iso], env=env, check=True,
        stdout=subprocess.DEVNULL,
    )
    controls = ["-display", "none", "-monitor", "none", "-serial", "mon:stdio" if manual else "stdio"]
    return command(arch, sector, image, controls=controls), env


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["image", "run", "demo"])
    parser.add_argument("image", type=Path)
    parser.add_argument("--sector", type=int, choices=[512, 4096], default=512)
    parser.add_argument("--historique-p8", action="store_true",
                        help="Utiliser le profil historique sans tmp sur une image P8 existante.")
    args = parser.parse_args()
    if args.historique_p8 and args.action != "run":
        parser.error("--historique-p8 exige run et une image existante")
    if args.action == "image":
        fixture_p9(args.image, args.sector)
    else:
        if args.action == "demo" and not args.image.exists():
            try:
                fixture_p9(args.image, args.sector)
            except FileExistsError:
                pass
        cmd, env = machine(os.environ.get("NOVA_ARCH", "x86_64"), args.sector, args.image, manual=True,
                           historique_p8=args.historique_p8)
        os.execvpe(cmd[0], cmd, env)
