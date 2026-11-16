"""Recette ARC-026 sur image P9 jetable. Une session par lot, langue et droits observables."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import time
from types import SimpleNamespace

from bloc import qmp
from cli import Terminal
from journalisation import arret, attend, sortie
from session_cli import HOST, fixture_p9

LOTS = {1: "pwd cat head tail wc more".split(), 2: "mkdir rmdir touch cp mv".split(),
        3: "help history clear ps top true false".split()}


def reread_transfer(out):
    def tool(action, *args):
        with (out/"session.raw").open("rb") as disk:
            return subprocess.run([HOST, action, "512", "512", *args], stdin=disk, capture_output=True)
    info = tool("inspect")
    info.check_returncode()
    slots = re.findall(rb"slot=(\d+) g.+?ration=(\d+) blocs=\d+ FS=(\w+) LV=(\w+)", info.stdout)
    assert len(slots) == 2
    slot, _, fs, lv = max(slots, key=lambda row: int(row[1]))
    tool("verify", fs, lv).check_returncode()
    results = {}
    for path in ("/documents/exemple.txt", "/--inconnue", "/--deplace", "/--copie", "/--rep", "/--absent"):
        result = tool("salvage", fs, lv, slot, path)
        if path in ("/documents/exemple.txt", "/--inconnue", "/--deplace"):
            result.check_returncode()
            assert result.stdout == b"NovaOS\n"
            results[path] = hashlib.sha256(result.stdout).hexdigest()
        else:
            assert result.returncode and b"NotFound" in result.stderr
            results[path] = "NotFound"
    (out/"relecture.json").write_text(json.dumps(results, indent=2))


def run(arch, lot, out):
    out.mkdir(parents=True, exist_ok=False)
    os.environ.update(NOVA_VCPUS="2", NOVA_RAM="256")
    image = out / "session.raw"
    fixture_p9(image, 512)
    records = []
    with ExitStack() as stack:
        client, peer = socket.socketpair()
        stack.enter_context(client)
        stack.enter_context(peer)
        channel = stack.enter_context(client.makefile("rwb", buffering=0))
        monitor = SimpleNamespace(stdin=channel, stdout=channel)
        vm = stack.enter_context(Terminal(arch, 512, image, f"arc026-{lot}", test=True,
                                 qmp_bloc=peer.fileno(), journal=out/"session.serial"))
        peer.close()
        qmp(monitor, "qmp_capabilities")
        vm.until(b"nsh> ")
        attend(lambda: sortie(vm, "service status 16"), lambda s: "RUNNING" in s)

        def command(cmd, rc=0):
            vm.vm.stdin.write(f"echo ARCBEGIN; {cmd}; echo ARCEND=$?\n".encode())
            vm.until(b"\r\nARCBEGIN\r\n")
            # history contient le texte « ARCEND=$? » des saisies précédentes.
            # Seule la ligne qui contient le statut développé acquitte la commande.
            text = vm.until(f"ARCEND={rc}\r\n".encode()).decode(errors="replace")
            # Le marqueur exécuté suffit ; une trace SMP peut couper le dessin de l’invite.
            records.append(dict(command=cmd, rc=rc, output=text))
            return text

        def snapshot():
            disks = qmp(monitor, "query-blockstats")
            stats, = [d["stats"] for d in disks if d["device"] == "d"]
            with image.open("rb") as f:
                return dict(sha256=hashlib.file_digest(f, "sha256").hexdigest(),
                            **{k: stats[k] for k in ("wr_operations", "wr_bytes", "flush_operations")})

        def manual(name, french):
            vm.vm.stdin.write(f"man {name}; echo ARCEND=$?\n".encode())
            frame = vm.until(b"q quitter" if french else b"q quit  / search")
            assert (name + " — ").encode() in frame
            vm.vm.stdin.write(b"q")
            vm.until(b"ARCEND=0\r\n")
            records.append(dict(command="man " + name, rc=0, output=frame.decode()))

        # Observe the final boot writes before arming the counters.
        before = snapshot()
        for _ in range(15):
            time.sleep(2)
            current = snapshot()
            if current == before:
                break
            before = current
        else:
            raise AssertionError("stockage occupé")
        for tag, french in (("fr-zzzzz", False), ("fr-zzzz", True), ("fr-CA", True)):
            command("export LANG=" + tag)
            for name in LOTS[lot]:
                help_text = command(name + " --help")
                assert (("Manuel : man " if french else "Manual: man ") + name) in help_text
                assert name + " — " in help_text and "DOC-IPC" not in help_text
                version = command(name + " --version")
                assert ("nova-cli" if lot <= 2 else "nsh") + " 0.20.0" in version
                for result in (help_text, version):
                    assert "DOC-SAMPLE" not in result and "\x1b[2J" not in result
                manual(name, french)
                command(name + " --inconnue", 3)
                if lot <= 2:
                    for flag in ("--help", "--version"):
                        result = command(f"@probe documentation {name} {flag}")
                        lang = "fr" if french else "en"
                        assert re.search(rf"DOC-ELF {name} pid=[1-9]\d* lang={lang} business=0", result)
                        assert (help_text.split(" — ")[1].splitlines()[0] in result if flag == "--help"
                                else "nova-cli 0.20.0" in result)
        after = snapshot()
        assert before == after, (before, after)
        command("cp /documents/exemple.txt /--inconnue")
        if lot == 1:
            for cmd, expected in [("cat -- --inconnue", "NovaOS"), ("more -- --inconnue", "NovaOS"),
                                  ("head -n 1 -- --inconnue", "NovaOS"), ("tail -c 3 -- --inconnue", "OS"),
                                  ("wc -c -- --inconnue", "7 --inconnue"), ("pwd -P", "/")]:
                assert expected in command(cmd)
            command("pwd -- --inconnue", 3)
        if lot == 2:
            command("mkdir -- --rep")
            command("touch -- --rep/--note")
            command("rmdir -- --rep", 28)
            command("cp -- --inconnue --copie")
            command("mv -- --copie --deplace")
            command("cat -- --copie", 20)
            command("cp -- --inconnue --deplace", 27)
            assert "NovaOS" in command("cat -- --deplace")
            command("rm -- --rep/--note")
            command("rmdir -- --rep")
            command("cd -- --rep", 20)
            command("touch -c -- --absent")
            command("cat -- --absent", 20)
            assert "NovaOS" in command("cat /documents/exemple.txt")
        if lot == 3:
            command("false", 1)
            command("true")
            assert "SUITE" in command("false || echo SUITE")
            assert "TRACE" in command("true && echo TRACE")
            assert "DOC-SAMPLE" in command("ps")
            assert "service" in command("help")
            assert "ARCBEGIN" in command("history")
            command("clear")
            vm.vm.stdin.write(b"top; echo TOPEND=$?\n")
            vm.until(b"q pour quitter")
            vm.vm.stdin.write(b"q")
            vm.until(b"TOPEND=0\r\n")
            records.append(dict(command="top (q)", rc=0))
            command("nsh -c 'history'", 5)
            command("nsh -c 'history --help'")
        control = snapshot()
        assert control["wr_operations"] > before["wr_operations"] and control["sha256"] != before["sha256"]
        arret(vm, 2)
    if lot == 2:
        reread_transfer(out)
    (out/"observations.json").write_text(json.dumps(dict(avant=before, apres=after, temoin=control,
                        observations=records), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=("aarch64", "x86_64"))
    parser.add_argument("lot", type=int, choices=LOTS)
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    run(args.arch, args.lot, args.out.resolve())
