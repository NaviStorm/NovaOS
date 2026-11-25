"""Inventaire Ethernet sur lien QEMU privé. Les deux noyaux doivent être construits."""
import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
import uuid

from materiel import ROOT, run


def utc():
    return datetime.now(timezone.utc).isoformat()


@contextmanager
def qualification(worktrees, report):
    lock = worktrees / ".qualification-utilitaires-exclusive.lock"
    owner = {"pid": os.getpid(), "token": uuid.uuid4().hex,
             "worktree": str(ROOT.parent), "action": "inventaire VirtIO-net"}
    start = time.monotonic()
    report["debut_attente"] = utc()
    while True:
        try:
            lock.mkdir()
            break
        except FileExistsError:
            time.sleep(2)
    report.update(acquisition=utc(), attente_s=time.monotonic() - start)
    marker = lock / "owner.json"
    try:
        marker.write_text(json.dumps(owner))
        processes = subprocess.check_output(["ps", "-axo", "comm="], text=True)
        if any("qemu-system-" in line or "hyperfine" in line for line in processes.splitlines()):
            raise RuntimeError("Charge incompatible sous verrou")
        yield
    finally:
        if marker.is_file() and json.loads(marker.read_text()) == owner:
            marker.unlink()
            lock.rmdir()
            report["liberation"] = utc()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path, help="nouveau répertoire durable")
    parser.add_argument("--files", action="store_true", help="exiger le témoin logiciel de la construction virtio-net-probe")
    args = parser.parse_args()
    archive = args.archive.resolve()
    archive.mkdir(parents=True, exist_ok=False)
    report = {"base_git": subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(), "essais": {}}
    try:
        images = {}
        for arch, target in (("x86_64", "x86_64-unknown-none"),
                             ("aarch64", "aarch64-unknown-none-softfloat")):
            directory = archive / arch
            directory.mkdir()
            elf = ROOT / "target" / target / "release/novaos"
            shutil.copyfile(elf, directory / "novaos.elf")
            iso = directory / "inventaire.iso"
            env = dict(os.environ, NOVA_ARCH=arch, NOVA_KERNEL_ELF=str(directory / "novaos.elf"))
            subprocess.run(["sh", ROOT / "scripts/iso.sh", "materiel-inventaire-qemu", iso],
                           check=True, env=env)
            images[arch] = iso
        with qualification(ROOT.parent.parent, report):
            for arch, iso in images.items():
                start = time.monotonic()
                result = report["essais"][arch] = {"reussi": False}
                try:
                    run(arch, iso, False, True, network=True, queue_probe=args.files,
                        directory=iso.parent, kernel=iso.parent / "novaos.elf")
                    result["reussi"] = True
                finally:
                    result["execution_s"] = time.monotonic() - start
    finally:
        (archive / "campagne.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    def interrupt(signum, frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupt)
    main()
