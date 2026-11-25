"""Reconstituer une variante du banc dans un nouveau répertoire hors worktree."""
import argparse
import gzip
import hashlib
import json
import runpy
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SERVICE = HERE.parents[2] / "user/novaauth/service"
BASE = "e471f369bf185988be8624cb422764701bd41a30"


def apply(root, patch, hashes):
    def verify(index):
        for name, expected in hashes.items():
            path = root / name
            actual = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None
            if actual != expected[index]:
                raise ValueError(f"Empreinte divergente pour {name}, étape {index}")

    verify(0)
    delta = gzip.decompress(patch.read_bytes())
    subprocess.run(["git", "apply", "--check", "-"], input=delta, cwd=root, check=True)
    subprocess.run(["git", "apply", "--whitespace=nowarn", "-"], input=delta, cwd=root, check=True)
    verify(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scenario", choices=["E2", "E7G", "E8G", "E8H"])
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    root = args.destination.resolve()
    if root.exists() or any((p / ".git").exists() for p in root.parents):
        parser.error("destination neuve requise en dehors de tout worktree")
    sys.path.insert(0, str(SERVICE))
    runpy.run_path(str(SERVICE / "profile.py"))["prepare"](
        root, source_ref=BASE, membership_r2=True, administration_ticket_vivant=True
    )
    exported = root / "novaOS/user/novaauth/service"
    selective = runpy.run_path(str(exported / "selective_exec_profile.py"))
    selective["install"](root)
    selective["install_file_preflight"](root)
    runpy.run_path(str(exported / "selective_old_ticket_profile.py"))["install"](root)
    historical = HERE.parent / "novaauth-r2-exec-g-duplex-20261125"
    original = json.loads((historical / "sources-export.json").read_text())
    apply(root, historical / "export-essai-e3.patch.gz", original["fichiers_avant_apres"])
    duplex = HERE.parent / "novaauth-r2-canaux-duplex-777ff38"
    sources = json.loads((duplex / "sources.json").read_text())
    apply(root, SERVICE / "duplex_fichiers_qemu.patch.gz",
          {k: (v["avant"], v["apres"]) for k, v in sources.items()})
    steps = ["raccord-drainage-mesures"]
    if args.scenario != "E2":
        steps.append("final-" + args.scenario.lower())
    for step in steps:
        apply(root, HERE / (step + ".patch.gz"),
              json.loads((HERE / (step + "-sources.json")).read_text()))
    print(f"Export {args.scenario} vérifié dans {root}")


if __name__ == "__main__":
    main()
