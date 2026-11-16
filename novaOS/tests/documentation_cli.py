"""Recette du catalogue natif. Lancer avec NOVA_DOC_TRACE=1 à la construction."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

from cli import Terminal
from journalisation import arret, attend
from session_cli import HOST, fixture_p9

PILOTS = ("nsh", "service", "journal", "ls", "rm", "cd", "vi", "man")
SERVICE = "help list status info dependencies policy start stop restart enable disable capabilities".split()
JOURNAL = "help list status show tail boot emit rotate retain purge".split()


def reread(image, present):
    def tool(*args):
        with image.open("rb") as data:
            return subprocess.run([HOST, *args], stdin=data, capture_output=True)
    records = tool("inspect", "512", "512")
    records.check_returncode()
    entries = re.findall(r"slot=(\d+) génération=(\d+) blocs=\d+ FS=(\w+) LV=(\w+)".encode(), records.stdout)
    slot, _, fs, lv = max(entries, key=lambda row: int(row[1]))
    tool("verify", "512", "512", fs, lv).check_returncode()
    result = {}
    for path in ("/voisin", "/--inconnue"):
        r = tool("salvage", "512", "512", fs, lv, slot, path)
        if path == "/voisin" or present:
            r.check_returncode()
            assert r.stdout == b"NovaOS\n"
            result[path] = hashlib.sha256(r.stdout).hexdigest()
        else:
            assert r.returncode and not r.stdout and b"NotFound" in r.stderr, (r.stdout, r.stderr)
            result[path] = {"code": r.returncode, "diagnostic": r.stderr.decode()}
    return result


def run(arch, out, cpus):
    out.mkdir(parents=True, exist_ok=False)
    os.environ["NOVA_VCPUS"] = str(cpus)
    os.environ["NOVA_RAM"] = "256"
    image = out / "refus.raw"
    fixture_p9(image, 512)
    records = []
    def command(vm, line, code=0):
        # La trace bloc SMP peut partager une ligne avec le statut. Le marqueur complet
        # contient un chiffre calculé par nsh et ne peut pas apparaître dans l’écho.
        vm.vm.stdin.write(f"{line}; echo __doc_rc_$?__fin\n".encode())
        text = vm.until(f"__doc_rc_{code}__fin".encode()).decode(errors="replace")
        vm.until(b"nsh> ")
        records.append(dict(commande=line, code=code, sortie=text))
        (out/"observations.json").write_text(json.dumps(records, ensure_ascii=False, indent=2))
        return text

    with Terminal(arch, 512, image, "documentation", journal=out/"ordinaire.serial") as vm:
        vm.until(b"nsh> ")
        vm.line("export LANG=fr-CA")
        attend(lambda: command(vm, "service status 9"), lambda s: "Current state: RUNNING" in s)
        assert "DOC-IPC SEND" in command(vm, "service info 9")
        command(vm, "service stop 9", 5)
        for name in PILOTS:
            help_text = command(vm, f"{name} --help")
            assert f"Manuel : man {name}" in help_text and "Exemples" in help_text
            version = command(vm, f"{name} --version")
            provider = "nova-cli" if name in ("vi","rm","ls") else "nsh"
            assert f"{provider} 0.20.0" in version
            for output in (help_text, version):
                assert "DOC-IPC SEND" not in output and "DOC-CD RESOLVE" not in output
            command(vm, f"{name} --inconnue", 3)
        for name, topics in (("service", SERVICE + ["capabilities restrict","capabilities restore"]), ("journal", JOURNAL)):
            for topic in topics:
                h = command(vm, f"{name} {topic} --help")
                assert f"{name} {topic}" in h and "DOC-IPC SEND" not in h
        assert "Current state: RUNNING" in command(vm, "service status storage-info")
        assert "1397118542" in command(vm, "service info storage-info")
        listing = command(vm, "service list")
        for num,name in [(1,"clock"),(2,"tmp-health"),(9,"storage-info"),(10,"cache"),(16,"journal")]:
            assert re.search(fr"service={name} id={num} etat=\w+ autostart=enabled", listing), listing
        command(vm,"mkdir -- /--version")
        assert "DOC-CD RESOLVE" in command(vm,"cd -- /--version")
        command(vm,"cd /")
        text = command(vm,"pwd; cd --version; echo cdrc=$?; pwd")
        assert "nsh 0.20.0" in text and "cdrc=0" in text and text.splitlines().count("/")==2 and "DOC-CD RESOLVE" not in text
        for path in ("/voisin", "/--inconnue"):
            command(vm,f"cp -- /documents/exemple.txt {path}")
        command(vm,"rm --help")
        assert "--inconnue" in command(vm,"rm --inconnue",3)
        assert "page absente" in command(vm,"man aucune-page",20)
        for page in (*PILOTS, "service capabilities restrict"):
            vm.vm.stdin.write(f"man {page}; echo pagercode=$?\n".encode())
            frame = vm.until(b"q quitter")
            expected = {"nsh": "Interpréter", "service": "Administrer", "journal": "Consulter",
                        "ls": "Lister", "rm": "Supprimer", "cd": "Changer", "vi": "Éditer",
                        "man": "Consulter", "service capabilities restrict": "Retirer un droit"}
            assert expected[page].encode() in frame
            if page == "rm":
                # Une occurrence unique au début doit être retrouvée après bouclage.
                vm.vm.stdin.write("/rm — Supprimer\r".encode())
                vm.until("trouvé".encode())
            if page == "service":
                vm.vm.stdin.write(b" ")
                page_down = vm.until(b"q quitter")
                assert not re.search(rb"man service 1/\d+", page_down)
                vm.vm.stdin.write(b"g")
                vm.until(b"q quitter")
                vm.vm.stdin.write(b"/plafond\r")
                vm.until("trouvé".encode())
            vm.vm.stdin.write(b"q")
            vm.until(b"pagercode=0")
            vm.until(b"nsh> ")
        command(vm,"ps")
        arret(vm,cpus)
    first = reread(image, True)
    removed = out/"retrait.raw"
    shutil.copyfile(image, removed)
    with Terminal(arch,512,removed,"retrait",journal=out/"retrait.serial",profil_config=True) as vm:
        vm.until(b"nsh> ")
        attend(lambda: command(vm, "service status 16"), lambda s: "Current state: RUNNING" in s)
        command(vm,"rm -- --inconnue")
        command(vm,"cat -- /--inconnue",20)
        assert "NovaOS" in command(vm,"cat -- /voisin")
        # Contrôle positif de la sonde STOP dans un profil qui détient ce droit.
        assert "DOC-IPC SEND STOP" in command(vm,"service stop 9")
        arret(vm,cpus)
    (out/"relecture.json").write_text(json.dumps([first,reread(removed,False)],indent=2))

if __name__ == "__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("arch",choices=("aarch64","x86_64"))
    parser.add_argument("out",type=Path)
    parser.add_argument("--cpus",type=int,default=2)
    a=parser.parse_args()
    run(a.arch,a.out.resolve(),a.cpus)
