"""Locale du shell, de son enfant natif et de son lecteur, dans la même session."""
import argparse
import json
import hashlib
import socket
import time
from contextlib import ExitStack
from types import SimpleNamespace
import os
import re
from pathlib import Path

from bloc import qmp
from cli import Terminal
from journalisation import arret, attend, sortie
from session_cli import fixture_p9

CASES = [(None, False), ("fr", True), ("FR", True), ("fr-FR", True), ("fr-CA", True),
         ("fr-BE", True), ("fr-CH", True), ("en-US", False), ("de-DE", False),
         ("es-ES", False), ("frr", False), ("fr--CA", False), ("zz", False)]


def run(arch, out):
    os.environ.update(NOVA_VCPUS="2", NOVA_RAM="256")
    out.mkdir(parents=True, exist_ok=False)
    image = out / "session.raw"
    fixture_p9(image, 512)
    cases = []
    with ExitStack() as stack:
        client, peer = socket.socketpair()
        stack.enter_context(client)
        stack.enter_context(peer)
        client.settimeout(15)
        channel = stack.enter_context(client.makefile("rwb", buffering=0))
        monitor = SimpleNamespace(stdin=channel, stdout=channel)
        vm = stack.enter_context(Terminal(arch, 512, image, "langues", test=True,
                                          qmp_bloc=peer.fileno(), journal=out/"langues.serial"))
        peer.close()
        qmp(monitor, "qmp_capabilities")
        vm.until(b"nsh> ")
        attend(lambda: sortie(vm, "service status 16"), lambda s: "Current state: RUNNING" in s)

        def snapshot():
            result = qmp(monitor, "query-blockstats")
            stats, = [d["stats"] for d in result if d["device"] == "d"]
            with image.open("rb") as disk:
                sha = hashlib.file_digest(disk, "sha256").hexdigest()
            return dict(sha256=sha, **{k: stats[k] for k in ("wr_operations", "wr_bytes",
                        "flush_operations", "failed_wr_operations", "failed_flush_operations")})

        def native(cmd):
            # Entrée interactive directe : le contrôle lui-même ne crée aucun enfant.
            vm.vm.stdin.write(f"echo DOCBEG; {cmd}; echo DOCEND=$?\n".encode())
            vm.until(b"\r\nDOCBEG\r\n")
            text = vm.until(b"DOCEND=0\r\n").decode()
            vm.until(b"nsh> ")
            return text

        # Le journal persiste encore les événements d’amorçage après son READY.
        # Armer la première fenêtre une fois le disque au repos et conserver ce témoin.
        idle = [snapshot()]
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            time.sleep(2)
            idle.append(snapshot())
            if idle[-1] == idle[-2]:
                break
        else:
            raise AssertionError(("stockage encore actif avant essai", idle))
        (out/"repos-initial.json").write_text(json.dumps(idle, indent=2))
        for tag, french in CASES:
            vm.line("unset LANG" if tag is None else f"export LANG={tag}")
            before = snapshot()
            stop = native("service stop --help")
            rm = native("rm --help")
            expected = "Supprimer les entrées" if french else "Remove explicitly named"
            assert expected in rm
            assert "DOC-ELF" not in rm
            assert ("Arrêter l’instance" if french else "Stop the current instance") in stop
            assert "DOC-IPC SEND" not in stop
            vm.vm.stdin.write(b"man rm; echo pagercode=$?\n")
            frame = vm.until(b"q quitter" if french else b"q quit  / search")
            assert expected.encode() in frame
            vm.vm.stdin.write(b"q")
            vm.until(b"pagercode=0")
            vm.until(b"nsh> ")
            public = snapshot()
            assert public == before, (tag, before, public)
            pilots = ("nsh", "service", "journal", "ls", "rm", "cd", "vi", "man")
            public_help = {}
            if tag in (None, "fr-CA"):
                for name in pilots:
                    text = native(f"{name} --help")
                    assert ("Manuel : man " if french else "Manual: man ") + name in text
                    public_help[name] = text
            public_versions = [native(f"{name} --version") for name in pilots]
            assert all("0.20.0" in text for text in public_versions)
            before_elf = snapshot()
            assert before_elf == public
            child = native("@probe documentation rm --help")
            child_version = native("@probe documentation rm --version")
            for result in (child, child_version):
                assert re.search(r"DOC-ELF rm pid=[1-9][0-9]* lang=" + ("fr" if french else "en")
                                 + r" business=0", result), result
            assert expected in child and "nova-cli 0.20.0" in child_version
            after = snapshot()
            assert after == before_elf, (tag, before_elf, after)
            cases.append(dict(avant=before, apres_aide=public, avant_elf=before_elf, apres=after,
                              aides=public_help, versions=public_versions, elf=child, elf_version=child_version,
                              locale=tag, langue="fr" if french else "en", service_stop_help=stop,
                              rm_help=rm, man_rm=frame.decode(), codes=[0, 0, 0]))
        # Témoin positif hors fenêtre de consultation : le compteur mesure bien notre disque.
        baseline = snapshot()
        native("touch preuve-compteur")
        changed = snapshot()
        assert changed["wr_operations"] > baseline["wr_operations"] and changed["sha256"] != baseline["sha256"]
        (out/"temoin-ecriture.json").write_text(json.dumps([baseline, changed]))
        assert "Current state: RUNNING" in sortie(vm, "service status 9")
        assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
        arret(vm, 2)
    (out/"langues.json").write_text(json.dumps(cases, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("arch", choices=("aarch64", "x86_64"))
    parser.add_argument("out", type=Path)
    a = parser.parse_args()
    run(a.arch, a.out.resolve())
