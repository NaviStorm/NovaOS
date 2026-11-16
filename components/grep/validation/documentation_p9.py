"""Essais documentaires uniquement. Chaque invocation exige un répertoire neuf."""
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import time
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT/"novaOS/tests"))
from cli import Terminal
from bloc import qmp
from journalisation import attend, sortie
from session_cli import fixture_p9


def main(arch, sector, out):
    os.environ.update(NOVA_VCPUS="2", NOVA_RAM="256")
    out.mkdir(parents=True, exist_ok=False)
    disk = out/"p9.raw"
    fixture_p9(disk, sector)
    rows = []
    with ExitStack() as stack:
        client, peer = socket.socketpair()
        stack.enter_context(client)
        stack.enter_context(peer)
        mon = SimpleNamespace(stdin=stack.enter_context(client.makefile("rwb", buffering=0)))
        mon.stdout = mon.stdin
        vm = stack.enter_context(Terminal(arch, sector, disk, "grep-doc", qmp_bloc=peer.fileno(), journal=out/"doc.serial"))
        peer.close()
        qmp(mon, "qmp_capabilities")
        vm.until(b"nsh> ")
        attend(lambda: sortie(vm, "service status 16"), lambda s: "Current state: RUNNING" in s)
        def snapshot():
            stats, = [v["stats"] for v in qmp(mon, "query-blockstats") if v["device"] == "d"]
            return [hashlib.sha256(disk.read_bytes()).hexdigest(), stats["wr_bytes"],
                    stats["wr_operations"], stats["flush_operations"]]
        before = snapshot()
        for _ in range(15):
            time.sleep(2)
            current = snapshot()
            if before == current:
                break
            before = current
        else:
            raise AssertionError("P9 encore actif")
        for tag, fr in [("en-US", False), ("fr-CA", True), ("fr--CA", False)]:
            vm.line(f"export LANG={tag}")
            desc = "Rechercher un motif" if fr else "Search for a pattern"
            for cmd, want in [("grep --help", desc), ("grep --version", "référence" if fr else "reference")]:
                result = sortie(vm, cmd)
                assert want in result, (tag, cmd, result)
                if cmd == "grep --help":
                    headings = (["Syntaxe :", "Sélection et interprétation des motifs", "Options diverses",
                                 "Contrôle de la sortie", "Contrôle du contexte", "Manuel : man grep"]
                                if fr else ["Usage:", "Pattern selection and interpretation:", "Miscellaneous:",
                                            "Output control:", "Context control:", "Manual: man grep"])
                    positions = [result.index(heading) for heading in headings]
                    assert positions == sorted(positions), (tag, headings)
                    assert "-E, --extended-regexp" in result and "-Z, --null" in result
                rows.append(dict(tag=tag, cmd=cmd, result=result))
            vm.vm.stdin.write(b"man grep; echo pagercode=$?\n")
            frame = vm.until(b"q quitter" if fr else b"q quit  / search")
            assert desc.encode() in frame, frame
            vm.vm.stdin.write(b"q")
            vm.until(b"pagercode=0")
            vm.until(b"nsh> ")
            rows.append(dict(tag=tag, cmd="man grep", result=frame.decode()))
        after = snapshot()
        assert before == after, (before, after)
    (out/"observations.json").write_text(json.dumps(dict(arch=arch, secteur=sector,
        avant=before, apres=after, consultations=rows), ensure_ascii=False, indent=1)+"\n")


if __name__ == "__main__":
    main(sys.argv[1], int(sys.argv[2]), Path(sys.argv[3]).resolve())
