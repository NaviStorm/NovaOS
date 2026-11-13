"""Arrêt de QEMU aux frontières du prototype, sur des images P9 jetables."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile

from cli import Terminal
import vi_volumes as volumes

TAILLE = 512 * 1024 + 128

def essai(arch, secteur, point, dossier):
    volumes.SIZES = (TAILLE,)
    volumes.BLOCKS = 1024
    image, ids = volumes.prepare(dossier, secteur)
    source = f"f{TAILLE}"
    destination = f"coupure-{point}"
    with Terminal(arch, secteur, image, "cow-groupe-" + point, test=True) as t:
        t.until(b"nsh> ", 180)
        t.vm.stdin.write(f"vi {source}\n".encode())
        t.until(b"\x1b[6n")
        t.vm.stdin.write(b"\x1b[24;80R")
        t.until(f"{source} [enregistré] 1:1".encode())
        t.vm.stdin.write(f"iX\x1b:w {destination}\r".encode())
        t.until("Saisir OUI puis Entrée : ".encode())
        t.vm.stdin.write(b"OUI\r")
        avant = t.until(b"COUPURE groupe " + point.encode(), 45)
        assert "fichier enregistré".encode() not in avant

    def h(op, *args, check=True):
        with image.open("rb") as f:
            return subprocess.run([volumes.HOST, op, str(volumes.BLOCKS), str(secteur), *args],
                                  stdin=f, capture_output=True, check=check)

    def verifier():
        h("verify", *ids)
        slots = re.findall(r"slot=(\d+) génération=(\d+)".encode(), h("inspect").stdout)
        actif = max(slots, key=lambda pair: int(pair[1]))[0]
        original = volumes.contents(TAILLE)
        assert h("salvage", *ids, actif, "/" + source).stdout == original
        dest = h("salvage", *ids, actif, "/" + destination, check=False)
        if point == "f2":
            assert dest.returncode == 0 and dest.stdout == b"X" + original
        else:
            assert dest.returncode != 0 and b"NotFound" in dest.stderr

    verifier()
    with Terminal(arch, secteur, image, "cow-reprise-" + point, test=True) as t:
        t.until(b"nsh> ", 180)
        t.exit()
    verifier()
    return dict(frontiere=point, octets_source=TAILLE, montage_normal=True, relecture_exacte=True,
                destination_complete=point == "f2", acquit_avant_coupure=False)

if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("architecture", choices=["aarch64", "x86_64"])
    p.add_argument("secteur", type=int, choices=[512, 4096])
    p.add_argument("rapport", type=Path)
    args = p.parse_args()
    essais = []
    for point in ("pages", "f1", "f2", "retrait"):
        with tempfile.TemporaryDirectory(prefix="nova-cow-groupe-") as dossier:
            essais.append(essai(args.architecture, args.secteur, point, Path(dossier)))
    args.rapport.write_text(json.dumps(essais, indent=2, ensure_ascii=False) + "\n")
