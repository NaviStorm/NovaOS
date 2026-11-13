"""Installer et relire nsh dans l'application UTM sur deux profils jetables."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import select
import subprocess
import sys
import time
import tty
import uuid

from dma_sequentiel import PROMPT, SERIE
from profils_utm import profil
from publication_utm_app import CTL, script
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from paquet_installation import systeme


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def nommer(paquet, phase):
    fichier = paquet / "config.plist"
    config = plistlib.loads(fichier.read_bytes())
    ident = str(uuid.uuid4())
    nom = "NovaOS-nsh-" + phase + "-" + ident[:8]
    config["Information"].update(Name=nom, UUID=ident)
    config["Serial"] = [dict(Mode="Ptty", Target="Auto")]
    fichier.write_bytes(plistlib.dumps(config))
    return paquet.rename(paquet.with_name(nom + ".utm")), ident


@contextmanager
def enregistrer(paquet, ident):
    machine = script(["set f to POSIX file (item 1 of argv)",
                      'tell application "UTM" to set vm to import new virtual machine from f',
                      'tell application "UTM" to return id of vm'], paquet)
    assert machine.lower() == ident.lower()
    try:
        yield machine
    except Exception:
        exporter(machine, paquet.with_name(paquet.stem + "-diagnostic.utm"))
        raise
    finally:
        # Seule la copie importée dont l'identité vient d'être vérifiée appartient au banc.
        script(['tell application "UTM" to delete (virtual machine id (item 1 of argv))'], machine)


class Console:
    def __init__(self, fd, journal):
        self.fd, self.journal, self.pending = fd, journal, b""

    def attendre(self, marker, timeout=900):
        fin = time.monotonic() + timeout
        while marker not in self.pending:
            if time.monotonic() >= fin:
                raise TimeoutError((marker, self.pending[-1400:]))
            if select.select([self.fd], [], [], .2)[0]:
                bloc = os.read(self.fd, 65536)
                if not bloc:
                    raise EOFError(marker)
                self.journal.write(bloc)
                self.journal.flush()
                self.pending += bloc
        before, _, self.pending = self.pending.partition(marker)
        return before + marker

    def ligne(self, commande, fin=b"nsh> "):
        os.write(self.fd, commande.encode()+b"\r")
        self.attendre(b"\r\n")
        return self.attendre(fin)


@contextmanager
def demarrer(machine, archive):
    subprocess.run([CTL, "start", machine], check=True, timeout=90)
    try:
        port = script(['tell application "UTM" to return address of first serial port of '
                       '(virtual machine id (item 1 of argv))'], machine)
        assert port.startswith(("/dev/tty", "/dev/pty"))
        fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
        try:
            tty.setraw(fd)
            with archive.open("xb") as journal:
                yield Console(fd, journal)
        finally:
            os.close(fd)
    finally:
        subprocess.run([CTL, "stop", machine, "--force"], check=True, timeout=60)


def exporter(machine, fichier):
    script(["set f to POSIX file (item 2 of argv)",
            'tell application "UTM" to export (virtual machine id (item 1 of argv)) to f'], machine, fichier)


def transfert(console, marque, sid):
    serie = console.attendre(marque)
    assert f"ATTACH sid={sid} domaine=2 bypass=0 BME=0".encode() in serie
    bilan = re.search(rb"NOVA DXE SORTIE droits=(\d+) retraits=(\d+) actifs=0 BME=0 bypass=0", serie)
    assert bilan and int(bilan[1]) == int(bilan[2]) > 0
    assert b"NovaOS v0.16.5" in serie and b"GICv3 groupe=1" in serie
    return serie


def script_vi(console):
    os.write(console.fd, b"vi recettes/edite.nsh\r")
    console.attendre(b"vi [NORMAL]")
    os.write(console.fd, "i#!/bin/nsh\recho 'Écrit dans vi'\r\x1b:wq\r".encode())
    console.attendre(b"nsh> ")
    assert "Écrit dans vi\r\n".encode() in console.ligne("./recettes/edite.nsh")
    os.write(console.fd, b"vi recettes/edite.nsh\r")
    console.attendre(b"vi [NORMAL]")
    os.write(console.fd, b":w recettes/autre.nsh\r")
    console.attendre("Saisir OUI puis Entrée : ".encode())
    os.write(console.fd, b"OUI\r")
    console.attendre("fichier enregistré".encode())
    os.write(console.fd, b":q\r")
    console.attendre(b"nsh> ")


VI_LINE = "1234567890abcdef" * 4
VI_LINES = 1024


def reouvrir_vi(console, name):
    debut = time.monotonic_ns()
    os.write(console.fd, f"vi {name}\r".encode())
    console.attendre(b"\x1b[6n")
    os.write(console.fd, b"\x1b[24;80R")
    console.attendre(f"{name} [enregistré] 1:1".encode())
    return (time.monotonic_ns() - debut) / 1e6


def grand_fichier(console):
    """La source de qualification est créée au clavier dans l'éditeur installé."""
    name = "recettes/volume.txt"
    os.write(console.fd, f"vi {name}\r".encode())
    console.attendre(f"{name} [nouveau] 1:1".encode())
    os.write(console.fd, b"i")
    for row in range(VI_LINES):
        os.write(console.fd, (VI_LINE+"\r").encode())
        console.attendre(f"{name} [modifié] {row+2}:1".encode())
    os.write(console.fd, b"\x1b:wq\r")
    console.attendre(b"nsh> ")
    ouverture_ms = reouvrir_vi(console, name)
    os.write(console.fd, b"iX\x1biY\x1buu")
    console.attendre(f"{name} [enregistré] 1:1".encode())
    os.write(console.fd, b"\x12\x12:w recettes/volume-edite.txt\r")
    console.attendre("Saisir OUI puis Entrée : ".encode())
    os.write(console.fd, b"OUI\r")
    console.attendre("fichier enregistré".encode())
    os.write(console.fd, b":q\r")
    console.attendre(b"nsh> ")
    return ouverture_ms


def verifier_persistance(console):
    transfert(console, b"nsh> ", 56)
    ouverture_ms = reouvrir_vi(console, "recettes/volume.txt")
    os.write(console.fd, b":q\r")
    console.attendre(b"nsh> ")
    assert b"NovaOS\r\n" in console.ligne("cat recettes/preuve.txt")
    assert b"recettes/preuve.txt" in console.ligne("ls -d -- **/*.txt")
    script = console.ligne("./recettes/preuve.nsh")
    assert "Lecture du disque système".encode() in script and b"NovaOS\r\n" in script
    for path in ("edite.nsh", "autre.nsh"):
        assert "Écrit dans vi\r\n".encode() in console.ligne("./recettes/" + path)
    for name, prefix in (("volume.txt", ""), ("volume-edite.txt", "YX")):
        content = (prefix + (VI_LINE+"\r\n")*VI_LINES).encode()
        assert console.ligne("cat recettes/"+name) == content + b"nsh> "
    assert b"controle-16" in console.ligne("nsh -c 'x=controle-16; echo $x'")
    console.ligne("exit", b"SYSTEME bilan Ok(())")
    return ouverture_ms


def qualifier(distribution, dest):
    dest.mkdir()
    support = distribution / "installateur.qcow2"
    original = digest(support)
    pret = profil(dest, distribution, support, True, installation=True)
    paquet, ident = nommer(pret, "installation")
    copie = dest / "installee.utm"
    with enregistrer(paquet, ident) as machine:
        with demarrer(machine, dest / "installation.serial") as console:
            selection = transfert(console, PROMPT.encode(), 48)
            assert b"DISQUE vierge verifie" in selection
            assert f"CIBLE serie={SERIE} sid=56 capacite=67108864 secteur=512".encode() in selection
            os.write(console.fd, f"AUTORISER {SERIE} 67108864\r".encode())
            console.attendre(b"INSTALLATION bilan Ok(())", 1800)
        exporter(machine, copie)
    assert digest(copie / "Data/source.qcow2") == original
    cible = digest(copie / "Data/cible.qcow2")
    systeme(copie)
    assert digest(copie / "Data/cible.qcow2") == cible
    assert all(d["Identifier"] != "source" for d in plistlib.loads((copie/"config.plist").read_bytes())["Drive"])
    (copie / "Data/source.qcow2").unlink()
    autonome, ident = nommer(copie, "systeme")
    with enregistrer(autonome, ident) as machine:
        with demarrer(machine, dest / "autonome.serial") as console:
            transfert(console, b"nsh> ", 56)
            assert b"NovaOS 0.16.5" in console.ligne("ls --version")
            assert b"/\r\nnsh> " in console.ligne("pw\t")
            assert b"NovaOS\r\nnsh> " in console.ligne("cat /syst\t")
            assert b"Nova\r\nnsh> " in console.ligne("head -c4 /systeme")
            assert b"cat /systeme" in console.ligne("history")
            assert b"nsh  vivant" in console.ligne("ps")
            console.ligne("top", b"q pour quitter")
            os.write(console.fd, b"q")
            console.attendre(b"nsh> ")
            for cmd in ["mkdir recettes", "cp /systeme recettes/preuve.txt", "cd recettes", "cd .."]:
                out = console.ligne(cmd)
                assert b"statut " not in out and b"erreur" not in out, (cmd, out)
            assert b"NovaOS 0.16.5" in console.ligne("rm --version recettes/preuve.txt recettes/preuve.txt")
            assert b"NovaOS\r\n" in console.ligne("cat **/*.txt")
            script = console.ligne("nsh /verification.nsh")
            assert "Lecture du disque système".encode() in script and b"NovaOS\r\n" in script
            out = console.ligne("nsh -c 'test -f /verification.nsh && cp /verification.nsh recettes/preuve.nsh'")
            assert b"statut " not in out and b"erreur" not in out
            script_vi(console)
            ouverture_ms = grand_fichier(console)
            console.ligne("exit", b"SYSTEME bilan Ok(())")
        with demarrer(machine, dest / "persistance.serial") as console:
            reouverture_ms = verifier_persistance(console)
    assert digest(support) == original
    return dict(application="UTM", profil="AArch64, CRB, GICv3 sans ITS, VirtIO QCOW2/512",
                serie="PTY de l’application", installation=True, support_sha256=original,
                disque_installe_sha256=cible, demarrages_autonomes=2, contenu_relu="NovaOS\n",
                ouverture_vi_ms=ouverture_ms, reouverture_apres_redemarrage_ms=reouverture_ms,
                octets_vi_relus=[(len(VI_LINE)+1)*VI_LINES, (len(VI_LINE)+1)*VI_LINES+2],
                scripts_vi_relus=["recettes/edite.nsh", "recettes/autre.nsh"],
                script_installe="/verification.nsh", script_execute_apres_redemarrage="./recettes/preuve.nsh")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("distribution", type=Path)
    p.add_argument("sortie", type=Path)
    args = p.parse_args()
    result = qualifier(args.distribution.resolve(), args.sortie.resolve())
    (args.sortie / "resultats.json").write_text(json.dumps(result, ensure_ascii=False, indent=2)+"\n")
