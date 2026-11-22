"""Contrôle préalable hors invité : profil et noyau exacts de l'ISO QEMU."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

def verify(iso, kernel):
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp:
        root = Path(tmp)
        for source, name in [("/limine.conf", "limine.conf"), ("/boot/novaos.elf", "kernel")]:
            subprocess.run(["xorriso", "-osirrox", "on", "-indev", str(iso),
                            "-extract", source, str(root / name)],
                           check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        config = (root / "limine.conf").read_text()
        commands = [line.strip() for line in config.splitlines()
                    if line.strip().startswith("cmdline:")]
        if commands != ["cmdline: novaauth-sessions"]:
            raise ValueError("profil ISO absent, ambigu ou incorrect")
        digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
        if digest(root / "kernel") != digest(kernel):
            raise ValueError("noyau ISO différent de l'artefact du banc")
        return {"profil": "novaauth-sessions", "iso_sha256": digest(iso),
                "kernel_sha256": digest(kernel), "limine_conf": config, "valide": True}

if __name__ == "__main__":
    print(json.dumps(verify(Path(sys.argv[1]), Path(sys.argv[2])), indent=2))
