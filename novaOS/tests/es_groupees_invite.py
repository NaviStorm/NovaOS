"""Exercer le rendu d'une commande et d'une recherche dans le vrai vi du prototype."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from cli import Terminal
from es_groupees_elargies import verifier
from vi_ecran import Ecran, cup


def essai(arch, secteur, generateur, dossier):
    image = dossier / "volume.raw"
    volume = json.loads(subprocess.check_output([generateur, "--image", image, str(secteur), "dense"]))
    with Terminal(arch, secteur, image, "es-groupees-invite", test=True) as t:
        envoyer = t.vm.stdin.write
        t.until(b"nsh> ")
        envoyer(b"vi source\r")
        t.until(b"\x1b[6n")
        envoyer(b"\x1b[24;80R")
        t.until("source [enregistré] 1:1".encode())
        t.until(cup(1, 1))
        ecran = Ecran(24, 80)

        def ouvrir(touche, mode):
            envoyer(touche)
            t.until(f"vi [{mode}]".encode())
            t.until(cup(24, 2))
            ecran.lines[-1] = touche.decode().ljust(80)

        def frappe(touche, attendu):
            envoyer(touche)
            flux = t.until(cup(24, len(attendu) + 1))
            assert ecran.feed(flux) == {23}, (flux, attendu)
            assert ecran.lines[-1].rstrip() == attendu
            assert (ecran.row, ecran.col) == (23, len(attendu))

        ouvrir(b"/", "RECHERCHE")
        prompt = "/"
        for c in "MIX":
            prompt += c
            frappe(c.encode(), prompt)
        frappe(b"\x08", "/MI")
        for prompt in ("/MIL", "/MILI", "/MILIE", "/MILIEU"):
            frappe(prompt[-1].encode(), prompt)
        envoyer(b"\r")
        t.until(b"16385:1")
        ouvrir(b":", "COMMANDE")
        frappe("é".encode(), ":é")
        frappe(b"\x08", ":")
        frappe(b"q", ":q")
        frappe(b"!", ":q!")
        envoyer(b"\r")
        t.until(b"nsh> ")
        t.exit()
    verifier(image, secteur, volume["blocs"], None)
    print(f"OK {arch}/{secteur} saisie et correction de l’invite, UTF-8, recherche et sortie")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64", "x86_64"))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("generateur", type=Path)
    args = p.parse_args()
    with tempfile.TemporaryDirectory(prefix="nova-invite-") as temp:
        essai(args.arch, args.secteur, args.generateur.resolve(), Path(temp))
