"""Essai du profil QCOW2/512 dans l'application UTM, avec son port série PTY."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import select
import subprocess
import time
import tty

from profils_utm import profil
from publication_conditionnelle import Cas, UTM, acquisition, argument, convertir, empreinte, neuf, pc

CTL = UTM / "MacOS/utmctl"


def script(lignes, *args):
    cmd = ["osascript", "-e", "on run argv"]
    for ligne in lignes:
        cmd += ["-e", ligne]
    return subprocess.check_output(cmd + ["-e", "end run", *map(str, args)], text=True, timeout=90).strip()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("banc", type=Path)
    p.add_argument("sortie", type=Path)
    a = p.parse_args()
    b = a.banc.resolve()
    out = a.sortie.resolve()
    out.mkdir()
    paquet = profil(out, b, b / "support/disque.raw", True, sequentiel=True)
    data = paquet / "Data"
    marque = neuf(out / "neuf.raw", 512)
    convertir(out / "neuf.raw", "raw", data / "cible.qcow2", "qcow2")
    cfg = plistlib.loads((paquet / "config.plist").read_bytes())
    nom = "NovaFS-PC1-" + marque[12:28].hex()[:12]
    cfg["Information"]["Name"] = nom
    cfg["QEMU"]["AdditionalArguments"] += ["-fw_cfg", argument(marque, 0, 0)]
    cfg["Serial"] = [dict(Mode="Ptty", Target="Auto")]
    (paquet / "config.plist").write_bytes(plistlib.dumps(cfg))
    paquet = paquet.rename(out / (nom + ".utm"))
    machine = script(["set f to POSIX file (item 1 of argv)",
                      'tell application "UTM" to set vm to import new virtual machine from f',
                      'tell application "UTM" to return id of vm'], paquet)
    assert machine.lower() == cfg["Information"]["UUID"].lower()
    (out / "uuid.txt").write_text(machine + "\n")
    subprocess.run([CTL, "start", machine], check=True, timeout=80)
    journal = bytearray()
    try:
        port = script(['tell application "UTM" to return address of first serial port of '
                       '(virtual machine id (item 1 of argv))'], machine)
        assert port.startswith("/dev/tty") or port.startswith("/dev/pty")
        fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
        try:
            tty.setraw(fd)
            reprise = False
            fin = time.monotonic() + 80
            while time.monotonic() < fin and b"P9LAB fin Ok(())" not in journal:
                if select.select([fd], [], [], .25)[0]:
                    journal.extend(os.read(fd, 65536))
                if b"PC1 PAUSE 0" in journal and not reprise:
                    os.write(fd, b"G")
                    reprise = True
        finally:
            os.close(fd)
    finally:
        (out / "console.log").write_bytes(journal)
        subprocess.run([CTL, "stop", machine, "--force"], check=True, timeout=30)
    assert b"PC1 ACK g=2 id=" + pc.identifiant(pc.C2).hex().encode() in journal and b"P9LAB fin Ok(())" in journal
    export = out / "relue.utm"
    script(["set f to POSIX file (item 2 of argv)",
            'tell application "UTM" to export (virtual machine id (item 1 of argv)) to f'], machine, export)
    assert empreinte(export / "Data/source.qcow2") == empreinte(paquet / "Data/source.qcow2")
    volume = acquisition(export/"Data/cible.qcow2", out/"apres.raw", Cas(512, "qcow2", "application"), marque)
    assert volume == pc.octets(pc.NOUVEAU)
    (out / "interface.pc1").write_bytes(volume)
    (out / "resultat.json").write_text(json.dumps(dict(nom=nom, uuid=machine, rang=2,
        sha256=pc.digest(volume).hex(), secteur=512, format="qcow2", arret="utmctl stop --force",
        source_qcow2=empreinte(paquet/"Data/source.qcow2"))) + "\n")


if __name__ == "__main__":
    main()
