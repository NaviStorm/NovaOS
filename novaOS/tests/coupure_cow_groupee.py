"""Arrêt de QEMU aux frontières du prototype, sur des images P9 jetables."""
import argparse
import json
import socket
from contextlib import ExitStack
from types import SimpleNamespace
from bloc import qmp
from pathlib import Path
import re
import subprocess
import tempfile
import time

from cli import Terminal
import vi_volumes as volumes
from es_groupees_elargies import grande_insertion

TAILLE = 512 * 1024 + 128

def essai(arch, secteur, point, dossier, taille=TAILLE, multipage=False):
    volumes.SIZES = (taille,)
    volumes.BLOCKS = 4096 if taille == 4 << 20 else 1024
    image, ids = volumes.prepare(dossier, secteur)
    source = f"f{taille}"
    destination = f"coupure-{point}"
    vol = point.startswith("vol-")
    stats = None
    with ExitStack() as pile:
        fd = None
        if vol:
            local, distant = socket.socketpair()
            pile.enter_context(local)
            pile.enter_context(distant)
            flux = pile.enter_context(local.makefile("rwb", buffering=0))
            mon = SimpleNamespace(stdin=flux, stdout=flux)
            fd = distant.fileno()
        t = pile.enter_context(Terminal(arch, secteur, image, "cow-groupe-" + point, test=True, qmp_bloc=fd))
        if vol:
            distant.close()
            qmp(mon, "qmp_capabilities")
        t.until(b"nsh> ", 180)
        t.vm.stdin.write(f"vi {source}\n".encode())
        t.until(b"\x1b[6n")
        t.vm.stdin.write(b"\x1b[24;80R")
        t.until(f"{source} [enregistré] 1:1".encode())
        touches = "\x10" if multipage else "iX\x1b"
        t.vm.stdin.write(f"{touches}:w {destination}\r".encode())
        t.until("Saisir OUI puis Entrée : ".encode())
        t.vm.stdin.write(b"OUI\r")
        avant = t.until(b"COUPURE groupe " + (b"armer" if vol else point.encode()), 45)
        if vol:
            def controle(commande):
                assert qmp(mon, "human-monitor-command", **{"command-line": f'qemu-io d "{commande}"'}) == ""
            controle("break flush_to_disk barriere")
            t.until(b"COUPURE groupe soumission", 30)
            # wait_break monopolise le moniteur ; laisser d’abord le lot soumettre
            # ses pages puis le FLUSH. Seul le retour de wait_break fait preuve.
            time.sleep(.5)
            controle("wait_break barriere")
            stats = next(d["stats"]["flush_operations"] for d in qmp(mon, "query-blockstats") if d["device"] == "d")
        assert "fichier enregistré".encode() not in avant

    def h(op, *args, check=True):
        with image.open("rb") as f:
            return subprocess.run([volumes.HOST, op, str(volumes.BLOCKS), str(secteur), *args],
                                  stdin=f, capture_output=True, check=check)

    insertion = grande_insertion() if multipage else b"X"

    def verifier():
        h("verify", *ids)
        slots = re.findall(r"slot=(\d+) génération=(\d+)".encode(), h("inspect").stdout)
        actif = max(slots, key=lambda pair: int(pair[1]))[0]
        original = volumes.contents(taille)
        assert h("salvage", *ids, actif, "/" + source).stdout == original
        dest = h("salvage", *ids, actif, "/" + destination, check=False)
        if point == "f2" or (point == "vol-f2" and dest.returncode == 0):
            assert dest.returncode == 0 and dest.stdout == insertion + original
        else:
            assert dest.returncode != 0 and b"NotFound" in dest.stderr

        return dest.returncode == 0

    publie = verifier()
    with Terminal(arch, secteur, image, "cow-reprise-" + point, test=True) as t:
        t.until(b"nsh> ", 180)
        t.exit()
    assert verifier() == publie
    return dict(frontiere=point, octets_source=taille, multipage=multipage, montage_normal=True, relecture_exacte=True,
                destination_complete=publie, flush_termines_au_point=stats, acquit_avant_coupure=False)

if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("architecture", choices=["aarch64", "x86_64"])
    p.add_argument("secteur", type=int, choices=[512, 4096])
    p.add_argument("rapport", type=Path)
    p.add_argument("--barrieres", action="store_true", help="requiert aussi io-0166-barrieres.patch")
    p.add_argument("--taille", type=int, choices=(TAILLE, 4 << 20), default=TAILLE)
    p.add_argument("--multipage", action="store_true", help="inclut la fixture de charge de 4 Mio")
    args = p.parse_args()
    essais = []
    points = ("pages", "f1", "f2", "retrait")
    if args.barrieres:
        points += ("avant-f1", "vol-f1", "avant-f2", "vol-f2")
    for point in points:
        with tempfile.TemporaryDirectory(prefix="nova-cow-groupe-") as dossier:
            essais.append(essai(args.architecture, args.secteur, point, Path(dossier), args.taille, args.multipage))
    args.rapport.write_text(json.dumps(essais, indent=2, ensure_ascii=False) + "\n")
