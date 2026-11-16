"""Retour du lecteur à l'éditeur, puis vérification au démarrage suivant."""
import argparse
import hashlib
import os
from pathlib import Path

from cli import Terminal
from journalisation import arret, sortie
from session_cli import fixture, fixture_p9


def run(arch, out):
    os.environ.update(NOVA_VCPUS="2", NOVA_RAM="256")
    out.mkdir(parents=True, exist_ok=False)
    disk = out / "temoin.raw"
    fixture_p9(disk, 4096)
    with Terminal(arch, 4096, disk, "manuel-vi", journal=out / "avant.serial") as vm:
        vm.until(b"nsh> ")
        vm.line("export LANG=fr-CA")
        for spelling in ("service help", "service --help"):
            help_text = sortie(vm, spelling)
            for item in ("service status SERVICE", "service capabilities restrict SERVICE CAPACITÉ", "Manuel : man service"):
                assert item in help_text
        vm.vm.stdin.write(b"man vi\n")
        vm.until(b"\x1b[6n")
        vm.vm.stdin.write(b"\x1b[12;48R")
        frame = vm.until(b"q quitter")
        assert b"\x1b[012;001H" in frame and b"\x1b[013;" not in frame
        vm.vm.stdin.write(b"\x1b[B")
        vm.until(b"q quitter")
        vm.vm.stdin.write(b"q")
        vm.until(b"nsh> ")
        vm.vm.stdin.write(b"vi manuel.txt\n")
        vm.until(b"vi [NORMAL]")
        vm.vm.stdin.write(b"ipreuve-man\r\x1b:wq\r")
        vm.until(b"nsh> ")
        assert "preuve-man" in sortie(vm, "cat manuel.txt")
        arret(vm, 2)
    with Terminal(arch, 4096, disk, "manuel-reprise", journal=out / "apres.serial") as vm:
        vm.until(b"nsh> ")
        assert "preuve-man" in sortie(vm, "cat manuel.txt")
        arret(vm, 2)

    historique = out / "historique-p8.raw"
    fixture(historique, 512)
    avant = hashlib.sha256(historique.read_bytes()).hexdigest()
    with Terminal(arch, 512, historique, "documentation-p8", historique_p8=True,
                  journal=out/"p8.serial") as vm:
        vm.until(b"nsh> ")
        assert "Remove explicitly named" in sortie(vm, "rm --help")
        vm.line("export LANG=fr-CA")
        assert "Supprimer les entrées" in sortie(vm, "rm --help")
        assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
        arret(vm, 2)
    apres = hashlib.sha256(historique.read_bytes()).hexdigest()
    assert avant == apres
    (out/"p8.sha256").write_text(f"avant {avant}\naprès {apres}\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("arch", choices=("aarch64", "x86_64"))
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    run(args.arch, args.out.resolve())
