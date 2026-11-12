"""Banc de saisie en profil ordinaire, avec relecture indépendante des octets P9."""
import os
from pathlib import Path
import re
import tempfile
import select
import subprocess

from cli import Terminal
from session_cli import fixture_p9, LONG, HOST


def trial(arch, sector, folder):
    image = folder / f"terminal-{sector}.raw"
    fixture_p9(image, sector)

    def read_file():
        with image.open("rb") as f:
            anchors = subprocess.check_output([HOST, "inspect", "512", str(sector)], stdin=f)
            fs, lv = re.search(rb"FS=(\w+) LV=(\w+)", anchors).groups()
            f.seek(0)
            subprocess.run([HOST, "verify", "512", str(sector), fs, lv], stdin=f, check=True)
            slots = re.findall(r"slot=(\d+) génération=(\d+)".encode(), anchors)
            slot = max(slots, key=lambda v: int(v[1]))[0]
            f.seek(0)
            return subprocess.check_output([HOST, "salvage", "512", str(sector), fs, lv, slot, "/lignes"], stdin=f)

    with Terminal(arch, sector, image, "terminal-stable") as term:
        term.until(b"nsh> ")
        send = term.vm.stdin.write

        def check(keys, marker):
            send(keys + b"\n")
            term.until(marker + b"nsh> ")

        check(b"pw\t", b"/\r\n")
        check(b"echo xy\t\x7f\x7fOK", b"OK\r\n")
        check(b"echo ac\x02\tb\x06d", b"abcd\r\n")
        check(b"cd doc\t", b"\r\n")
        check(b"pwd", b"/documents\r\n")
        check(b"touch 'notes 2026.txt'", b"\r\n")
        check(b'echo "notes 2\t', b"notes 2026.txt\r\n")
        check(b"echo notes\\ 2\t", b"notes 2026.txt\r\n")
        send(b"c\t\t")
        term.until(b"clear\r\n")
        check(b"\x7f" + b"echo multiples", b"multiples\r\n")
        for command in (b"cat", b"head -n99", b"tail", b"more"):
            send(command + b" /lignes\n")
            if command == b"more":
                # La première ligne très longue peut déclencher plusieurs pauses.
                while b"fin\r\nnsh> " not in term.pending:
                    if b"--More--" in term.pending:
                        term.until(b"--More--")
                        send(b" ")
                    if not select.select([term.vm.stdout], [], [], 20)[0]:
                        raise TimeoutError(term.pending[-300:])
                    data = os.read(term.vm.stdout.fileno(), 65536)
                    assert data
                    term.log.write(data)
                    term.pending += data
            term.until(b"fin\r\nnsh> ")
        send(b"history\n")
        term.until(b"1  pwd\r\n")
        term.until(b"nsh> ")
        check(b"clear", b"\x1b[2J\x1b[H")
        send(b"help\n")
        term.until(b"top touch true")
        term.until(b"nsh> ")
        send(b"ps\n")
        snapshot = term.until(b"nsh> ")
        rows = re.findall(rb"(\d+)  nsh  vivant  (\d+)", snapshot)
        assert len(rows) == 1 and int(rows[0][1]) > 0
        send(b"top\n")
        for _ in range(2):
            term.until(b"q pour quitter")
        send(b"q")
        term.until(b"\x1b[?1049l")
        assert term.until(b"nsh> ") == b"nsh> "
        term.exit()
    assert read_file() == LONG
    print(f"{arch}/{sector} — édition, complétion, quatre lecteurs, historique, écran, ps/top, contenu P9 ; nsh {rows[0][1].decode()} Kio", flush=True)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="nsh-terminal-") as tmp:
        for sector in (512, 4096):
            trial(os.environ["NOVA_ARCH"], sector, Path(tmp))
