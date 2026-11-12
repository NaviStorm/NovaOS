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
    assert b"NovaOS v0.15.1" in serie and b"GICv3 groupe=1" in serie
    return serie


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
            assert b"NovaOS 0.15.1" in console.ligne("ls --version")
            for cmd in ["mkdir recettes", "cp /systeme recettes/preuve.txt", "cd recettes", "cd .."]:
                out = console.ligne(cmd)
                assert b"statut " not in out and b"erreur" not in out, (cmd, out)
            assert b"NovaOS 0.15.1" in console.ligne("rm --version recettes/preuve.txt recettes/preuve.txt")
            assert b"NovaOS\r\n" in console.ligne("cat **/*.txt")
            console.ligne("exit", b"SYSTEME bilan Ok(())")
        with demarrer(machine, dest / "persistance.serial") as console:
            transfert(console, b"nsh> ", 56)
            assert b"NovaOS\r\n" in console.ligne("cat recettes/preuve.txt")
            assert b"recettes/preuve.txt" in console.ligne("ls -d -- **/*.txt")
            console.ligne("exit", b"SYSTEME bilan Ok(())")
    assert digest(support) == original
    return dict(application="UTM", profil="AArch64, CRB, GICv3 sans ITS, VirtIO QCOW2/512",
                serie="PTY de l’application", installation=True, support_sha256=original,
                disque_installe_sha256=cible, demarrages_autonomes=2, contenu_relu="NovaOS\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("distribution", type=Path)
    p.add_argument("sortie", type=Path)
    args = p.parse_args()
    result = qualifier(args.distribution.resolve(), args.sortie.resolve())
    (args.sortie / "resultats.json").write_text(json.dumps(result, ensure_ascii=False, indent=2)+"\n")
