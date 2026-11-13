"""Qualification de vi sur une image P9 privée créée avec es-groupees --image."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

from cli import Terminal, HOST
from vi_ipc_sauvegarde import bilan

TAILLE = 4 << 20
SEQUENCE = b"A" * 126 + b"Z\n"
MILIEU = TAILLE // 2
CAS = ("debut", "milieu", "fin", "multiple", "fragmente", "sature", "index")


def original():
    contenu = bytearray(SEQUENCE * (TAILLE // len(SEQUENCE)))
    contenu[MILIEU:MILIEU + 6] = b"MILIEU"
    return bytes(contenu)


def operation(cas, envoyer, attendre):
    source = original()
    if cas in ("milieu", "multiple"):
        envoyer("/MILIEU\r")
        attendre("source [enregistré] 16385:1")
        envoyer("xxié\x1b")
        attendu = source[:MILIEU] + "é".encode() + source[MILIEU + 2:]
        if cas == "multiple":
            envoyer("ggiPRE\x1bGiFIN\x1b")
            attendu = b"PRE" + attendu + b"FIN"
        return attendu
    if cas == "index":
        for touches, ligne in [("G", 32769), ("gg", 1), ("G", 32769), ("gg", 1)]:
            envoyer(touches)
            attendre(f"source [enregistré] {ligne}:1")
    if cas == "fin":
        envoyer("GiFIN\x1b")
        return source + b"FIN"
    envoyer("iX\x1b")
    return b"X" + source


def verifier(image, secteur, blocs, attendu, ids=("61"*16, "62"*16), tiers=None):
    def lire(op, *args, check=True):
        with image.open("rb") as f:
            return subprocess.run([HOST, op, str(blocs), str(secteur), *args],
                                  stdin=f, capture_output=True, check=check)
    lire("verify", *ids)
    generations = re.findall(r"slot=(\d+) génération=(\d+)".encode(), lire("inspect").stdout)
    assert len(generations) == 2
    slot = max(generations, key=lambda v: int(v[1]))[0]
    assert lire("salvage", *ids, slot, "/source").stdout == original()
    if tiers is not None:
        contenu = lire("salvage", *ids, slot, "/tiers").stdout
        assert len(contenu) == tiers and contenu == b"S"*tiers
    dest = lire("salvage", *ids, slot, "/copie", check=False)
    if attendu is None:
        assert dest.returncode and b"NotFound" in dest.stderr
    else:
        assert dest.returncode == 0 and dest.stdout == attendu
    return sorted(int(g) for _, g in generations)


def essai(a, secteur, cas, generateur, dossier, sonde=True):
    image = dossier/"volume.raw"
    profil = cas if cas in ("fragmente", "sature") else "dense"
    etat = json.loads(subprocess.check_output([generateur, "--image", image, str(secteur), profil]))
    tiers = etat.get("octets_tiers")
    avant = verifier(image, secteur, etat["blocs"], None, tiers=tiers)
    temoin = None
    if cas == "sature":
        # L'ouverture seule peut publier atime ; comparer à une copie de la même image.
        clone = dossier/"temoin.raw"
        shutil.copyfile(image, clone)
        with Terminal(a, secteur, clone, "elargi-temoin-lecture", test=True) as ctl:
            ctl.until(b"nsh> ")
            ctl.vm.stdin.write(b"vi source\n")
            ctl.until(b"\x1b[6n")
            ctl.vm.stdin.write(b"\x1b[24;80R")
            ctl.until("source [enregistré] 1:1".encode())
            ctl.vm.stdin.write(b"iX\x1b:q!\r")
            ctl.until(b"nsh> ")
            ctl.exit()
        temoin = verifier(clone, secteur, etat["blocs"], None, tiers=tiers)
    with Terminal(a, secteur, image, "elargi-"+cas, test=True) as t:
        def send(s):
            t.vm.stdin.write(s.encode())
        def wait(s):
            return t.until(s.encode(), 100, refus=b"Panique du noyau")
        wait("nsh> ")
        send("vi source\n")
        wait("\x1b[6n")
        send("\x1b[24;80R")
        wait("source [enregistré] 1:1")
        attendu = operation(cas, send, wait)
        # Les dernières frappes précèdent la fenêtre chronométrée.
        send(":")
        wait("vi [COMMANDE] source [modifié]")
        depart = time.monotonic_ns()
        send("w copie\r")
        wait("Saisir OUI puis Entrée : ")
        oui = time.monotonic_ns()
        send("OUI\r")
        if cas == "sature":
            wait("échec de l’opération")
            attendu = None
        else:
            wait("fichier enregistré")
        fin = time.monotonic_ns()
        send(":q!\r")
        wait("nsh> ")
        t.exit()
    apres = verifier(image, secteur, etat["blocs"], attendu, tiers=tiers)
    if attendu is None:
        assert apres == temoin, (avant, temoin, apres)
    text = t.archive.read_text()
    row = dict(octets=TAILLE, transfert_avant_oui_ms=(oui-depart)/1e6,
               publication_apres_oui_ms=(fin-oui)/1e6)
    compte = {}
    if attendu is not None and sonde:
        mesure = dossier/"mesure.json"
        mesure.write_text(json.dumps([row]))
        compte = bilan(t.archive, mesure)
    if not sonde:
        assert "SAVE from=" not in text
    pic_cadres = int(re.search(r"BLOC memoire [^\n]*pic_cadres=(\d+)", text)[1])
    mem = {}
    for pid, pages, pile in re.findall(r"CLI mem pid=(\d+) privees=(\d+) pile=(\d+)", text):
        mem[pid] = max(mem.get(pid, 0), 4*(int(pages)+int(pile)))
    return dict(architecture=a, secteur=secteur, cas=cas, **etat, avant=avant, apres=apres, temoin_lecture=temoin,
                pic_cadres_session=pic_cadres, sonde=sonde, memoire_observee_kio=mem, chemin=(
                    "refuse" if attendu is None else "non_instrumente" if not sonde else
                    "groupe" if "lots" in compte else "historique"),
                sauvegarde_ms=round((fin-depart)/1e6,3), compteur=compte,
                sha256_destination=hashlib.sha256(attendu).hexdigest() if attendu else None)


if __name__ == "__main__":
    arg = argparse.ArgumentParser(description=__doc__)
    arg.add_argument("arch")
    arg.add_argument("secteur", type=int)
    arg.add_argument("cas", choices=CAS)
    arg.add_argument("generateur", type=Path)
    arg.add_argument("rapport", type=Path)
    arg.add_argument("--sans-sonde", action="store_true")
    args = arg.parse_args()
    with tempfile.TemporaryDirectory(prefix="nova-elargi-") as d:
        r = essai(args.arch, args.secteur, args.cas, args.generateur.resolve(), Path(d), not args.sans_sonde)
    args.rapport.write_text(json.dumps(r, indent=2, ensure_ascii=False)+"\n")
