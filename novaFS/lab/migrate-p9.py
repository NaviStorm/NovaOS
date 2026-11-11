#!/usr/bin/env python3
"""Publie exclusivement une nouvelle image P9 après relecture du fichier de travail."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile


def digest(stream):
    stream.seek(0)
    return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--outil", required=True, type=Path)
    parser.add_argument("--secteur", type=int, choices=(512, 4096), required=True)
    parser.add_argument("--profil", required=True, choices=("contenu", "catalogue-p0", "p5-p6", "p7"))
    parser.add_argument("--blocs-destination", type=int)
    parser.add_argument("--empreinte", help="SHA-256 de confiance du catalogue clair")
    parser.add_argument("--fs")
    parser.add_argument("--lv")
    args = parser.parse_args()
    if not stat.S_ISREG(args.source.lstat().st_mode):
        parser.error("la source doit être un fichier image ordinaire")
    if os.path.lexists(args.destination):
        parser.error("la destination existe déjà")
    # O_NOFOLLOW maintient le refus des liens au moment de l'ouverture.
    with os.fdopen(os.open(args.source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as src:
        metadata = os.fstat(src.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size % 4096:
            parser.error("géométrie du fichier source")
        blocks = metadata.st_size // 4096
        target = blocks if args.blocs_destination is None else args.blocs_destination
        if not 1 <= blocks <= 32768 or not 1 <= target <= 32768:
            parser.error("géométrie hors du profil de l'outil")
        commands = {"contenu": "migrate-content-p9", "catalogue-p0": "migrate-ready-p9",
                    "p5-p6": "migrate-clear-p9", "p7": "migrate-p9"}
        cmd = [str(args.outil.resolve()), commands[args.profil], str(blocks), str(args.secteur)]
        fds = ()
        before = digest(src)
        if args.profil == "contenu":
            if not args.fs or not args.lv or blocks != target:
                parser.error("le contenu autonome exige --fs, --lv et une géométrie identique")
            cmd += [args.fs, args.lv]
        elif args.profil == "p7":
            cmd += [str(target), "--cles-fd3"]
            fds = (3, 4)
        else:
            if args.empreinte != before:
                parser.error("l'empreinte de confiance ne correspond pas à l'image source")
            cmd += [str(target), before]
            fds = (4,)
        src.seek(0)
        directory = args.destination.parent
        space = os.statvfs(directory)
        if space.f_bavail * space.f_frsize < target * 4096:
            parser.error("espace disponible insuffisant pour l'image P9 complète")
        with tempfile.NamedTemporaryFile(dir=directory, prefix=".novafs-p9-") as work:
            result = subprocess.run(cmd, stdin=src, stdout=work, stderr=subprocess.PIPE, pass_fds=fds)
            if result.returncode:
                raise RuntimeError(result.stderr.decode(errors="replace").strip())
            receipt = json.loads(result.stderr.decode().splitlines()[-1])
            if receipt.get("migration_verifiee") is not True or os.fstat(work.fileno()).st_size != target * 4096:
                raise ValueError("publication incomplète")
            if digest(work) != receipt["sha256"] or receipt["sha256_source"] != before or digest(src) != before:
                raise ValueError("l'image a changé avant la publication")
            # Sur Darwin F_FULLFSYNC demande la persistance au support hôte.
            fcntl.fcntl(work.fileno(), fcntl.F_FULLFSYNC)
            dirfd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.link(work.name, args.destination)  # échoue atomiquement si le nom existe
                try:
                    os.fsync(dirfd)
                except OSError as exc:
                    raise RuntimeError("issue de publication du nom incertaine, vérifier la destination") from exc
            finally:
                os.close(dirfd)
            print(json.dumps(receipt, ensure_ascii=False))


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError) as error:
        sys.exit(str(error))
