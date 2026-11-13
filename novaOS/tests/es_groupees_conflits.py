"""Conflits vi sur des fichiers ouverts dans le vrai courtier de qualification."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile
from cli import Terminal, HOST
from vi_pages import fixture, DENSE, EDGES


def essai(arch, secteur, dossier):
    disque, ids = fixture(dossier, secteur)
    with Terminal(arch, secteur, disque, "course-publication", test=True) as t:
        def ecrire(s):
            t.vm.stdin.write(s.encode())
        def attendre(s):
            return t.until(s.encode(), 60)
        def ouvrir(cmd, nom):
            ecrire(cmd+"\n")
            attendre("\x1b[6n")
            ecrire("\x1b[24;80R")
            attendre(nom+" [enregistré] 1:1")
        attendre("nsh> ")
        for action in ["iZ\x1bG", "iY\x1b:w jamais\r", "iK\x1b:w renom-test\r"]:
            ouvrir("@probe vi-concurrence", "vi-essai.nsh")
            ecrire(action)
            assert b"Saisir OUI" not in attendre("conflit : la source a changé")
            ecrire(":q\r")
            attendre("modifications non sauvegardées")
            ecrire(":q!\r")
            attendre("nsh> ")
        # Source valide avec un caractère UTF-8/CRLF hors de la première page :
        # la voie ASCII du prototype peut refuser ; ne pas compter cela comme un succès.
        ouvrir("vi bords", "bords")
        ecrire("iZ\x1b:w copie-bords\r")
        attendre("Saisir OUI puis Entrée : ")
        ecrire("OUI\r")
        # Pour éviter de confondre un redraw avec le résultat, attendre la prochaine
        # interrogation de taille, puis identifier le message de l'éditeur.
        rendu = t.until(b"\x1b[6n", 60)
        succes = "fichier enregistré".encode() in rendu
        refuse = b"argument invalide" in rendu
        assert succes != refuse, rendu[-500:]
        publie = succes
        if refuse:
            ecrire(":q\r")
            attendre("modifications non sauvegardées")
        ecrire(":q!\r")
        attendre("nsh> ")
        t.exit()
    def lire(op, *args, check=True):
        with disque.open("rb") as d:
            return subprocess.run([HOST, op, "1024", str(secteur), *args],stdin=d,capture_output=True,check=check)
    lire("verify", *ids)
    slots = re.findall(r"slot=(\d+) génération=(\d+)".encode(), lire("inspect").stdout)
    slot = max(slots,key=lambda g:int(g[1]))[0]
    tiers = bytearray(DENSE)
    tiers[4096], tiers[12345] = ord("!"), ord("?")
    for nom, attendu in [("source-deplacee", tiers), ("vi-essai.nsh", b"tiers"), ("bords", EDGES)]:
        assert lire("salvage", *ids, slot, "/"+nom).stdout == attendu
    for nom in ["jamais", "renom-test", *([] if publie else ["copie-bords"])]:
        r = lire("salvage", *ids, slot, "/"+nom, check=False)
        assert r.returncode and b"NotFound" in r.stderr
    if publie:
        assert lire("salvage", *ids, slot, "/copie-bords").stdout == b"Z"+EDGES
    return dict(architecture=arch, secteur=secteur, ecriture_concurrente=True, suppression_ouverte_refusee=True,
                remplacement_du_nom_detecte=True, modifications_locales_conservees=True, bords_utf8_publies=publie)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch")
    p.add_argument("secteur", type=int)
    p.add_argument("rapport", type=Path)
    a = p.parse_args()
    with tempfile.TemporaryDirectory() as d:
        res = essai(a.arch, a.secteur,Path(d))
    a.rapport.write_text(json.dumps(res,ensure_ascii=False,indent=2)+"\n")
