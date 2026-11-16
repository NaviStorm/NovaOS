"""Recette ARC-026 sur image P9 jetable. Une session par lot, langue et droits observables."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import time
from types import SimpleNamespace

from bloc import qmp
from cli import Terminal
from journalisation import arret, attend, sortie
from session_cli import fixture_p9

LOTS = {1: "pwd cat head tail wc more".split()}


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
            data = vm.until(b"ARCEND=") + vm.until(b"\r\n")
            text = data.decode(errors="replace")
            assert text.rstrip().endswith(f"ARCEND={rc}"), (cmd, text)
            vm.until(b"nsh> ")
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
            vm.until(b"nsh> ")
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
        control = snapshot()
        assert control["wr_operations"] > before["wr_operations"] and control["sha256"] != before["sha256"]
        arret(vm, 2)
    (out/"observations.json").write_text(json.dumps(dict(avant=before, apres=after, temoin=control,
                        observations=records), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=("aarch64", "x86_64"))
    parser.add_argument("lot", type=int, choices=LOTS)
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    run(args.arch, args.lot, args.out.resolve())
