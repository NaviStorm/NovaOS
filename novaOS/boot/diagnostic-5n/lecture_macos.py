"""Lecture d'une image régulière par le FAT natif de macOS, en montage imposé en lecture seule."""

import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import re
import subprocess
import tempfile
import zipfile

from support import require


def main(image, archive):
    require(image.is_file() and not image.is_symlink(), "Image régulière requise")
    with tempfile.TemporaryDirectory() as tmp:
        p = subprocess.run(
            [
                "hdiutil",
                "attach",
                "-readonly",
                "-nobrowse",
                "-noautoopen",
                "-noautofsck",
                "-mountroot",
                tmp,
                "-plist",
                str(image.resolve()),
            ],
            check=True,
            stdout=subprocess.PIPE,
        )
        entries = plistlib.loads(p.stdout)["system-entities"]
        whole = next(
            e["dev-entry"]
            for e in entries
            if re.fullmatch(r"/dev/disk[0-9]+", e.get("dev-entry", ""))
        )
        try:
            (log,) = [
                e for e in entries if Path(e.get("mount-point", "/")).name == "NOVALOGS"
            ]
            root = Path(log["mount-point"]).resolve()
            require(
                root.parent == Path(tmp).resolve(), "Montage hors du répertoire privé"
            )
            info = plistlib.loads(
                subprocess.check_output(
                    ["diskutil", "info", "-plist", log["dev-entry"]]
                )
            )
            require(
                info["ParentWholeDisk"] == Path(whole).name
                and not info["Writable"]
                and info["FilesystemType"] == "msdos",
                "Montage FAT en lecture seule attendu",
            )
            files = {}
            with zipfile.ZipFile(archive) as z:
                for n in z.namelist():
                    require(
                        re.fullmatch(
                            r"BOOT000[1-8]/(rapport\.json|JOURNAL\.TXT|VALIDES\.SHA)", n
                        ),
                        "Nom inattendu",
                    )
                    data = (root / n).read_bytes()
                    require(data == z.read(n), "Fichier FAT divergent")
                    files[n] = hashlib.sha256(data).hexdigest()
            require(len(files) == 24, "Suite de huit rapports incomplète")
            return dict(lecteur="macOS msdos", lecture_seule=True, fichiers=files)
        finally:
            subprocess.run(
                ["hdiutil", "detach", whole], stdout=subprocess.PIPE, check=True
            )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("image", type=Path)
    p.add_argument("rapports", type=Path)
    args = p.parse_args()
    print(json.dumps(main(args.image, args.rapports), indent=2))
