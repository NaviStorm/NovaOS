"""QEMU arrêté après F2 du premier lot, avant réponse et avant publication.
Requiert la fixture io-0165-coupure.patch uniquement dans le service de laboratoire.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile

from vi_pages import EDGES, HOST, Terminal, fixture


def verifier(image, secteur, ids):
    def h(op, *params, check=True):
        with image.open("rb") as f:
            return subprocess.run([HOST, op, "1024", str(secteur), *params],
                                  stdin=f, capture_output=True, check=check)
    h("verify", *ids)
    slots = re.findall(r"slot=(\d+) génération=(\d+)".encode(), h("inspect").stdout)
    actif = max(slots, key=lambda item: int(item[1]))[0]
    assert h("salvage", *ids, actif, "/bords").stdout == EDGES
    refus = h("salvage", *ids, actif, "/resultat", check=False)
    assert refus.returncode != 0 and b"NotFound" in refus.stderr


def essai(arch, secteur, dossier):
    image, ids = fixture(dossier, secteur)
    with Terminal(arch, secteur, image, "copie-plage-coupure", test=True) as t:
        t.until(b"nsh> ", 300)
        t.vm.stdin.write(b"vi bords\n")
        t.until(b"\x1b[6n")
        t.vm.stdin.write(b"\x1b[24;80R")
        t.until("bords [enregistré] 1:1".encode())
        t.vm.stdin.write(b"iZ\x1b:w resultat\r")
        t.until("Saisir OUI puis Entrée : ".encode())
        t.vm.stdin.write(b"OUI\r")
        t.until(b"COUPURE COPY_RANGE lot durable", 180)
        # Le service est suspendu après commit. La sortie de contexte tue QEMU.
    verifier(image, secteur, ids)
    with Terminal(arch, secteur, image, "copie-plage-reprise", test=True) as t:
        t.until(b"nsh> ", 300)
        t.line("cat resultat", status="sortie 20", has=["introuvable"])
        t.exit()
    verifier(image, secteur, ids)
    return dict(architecture=arch, secteur=secteur, coupure="après F2 du premier lot",
                remontage_normal=True, octets_source_relus=len(EDGES), nom_final_absent=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("architecture", choices=["aarch64", "x86_64"])
    parser.add_argument("secteur", type=int, choices=[512, 4096])
    parser.add_argument("rapport", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="nova-coupure-plage-") as d:
        result = essai(args.architecture, args.secteur, Path(d))
    args.rapport.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
