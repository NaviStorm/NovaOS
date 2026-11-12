"""Ouvre, modifie et publie quatre vrais fichiers P9 dans un invité NovaOS."""
import json
import os
import re
import subprocess
import tempfile
import time
from pathlib import Path
from cli import HOST, ROOT, Terminal

SIZES = (8192, 65536, 1048576, 4194304)
BLOCKS = 4096
LINE = b"A" * 126 + b"Z\n"


def contents(n):
    return LINE * (n // len(LINE))


def prepare(folder, sector):
    commands = []
    for size in SIZES:
        commands.append(f"create /f{size}")
        data = contents(size)
        for off in range(0, size, 65536):
            commands.append(f"write /f{size} {off} {data[off:off+65536].hex()}")
    base, image = folder/"base.raw", folder/"p9.raw"
    with base.open("xb") as out:
        r = subprocess.run([HOST, "script", str(BLOCKS), str(sector)],
                           input=("\n".join(commands)+"\n").encode(), stdout=out, stderr=subprocess.PIPE)
        if r.returncode:
            raise ValueError(r.stderr[-1024:])
    ids = re.search(rb"FS=(\w+) LV=(\w+)", r.stderr).groups()
    with base.open("rb") as src, image.open("xb") as out:
        subprocess.run([HOST, "migrate-content-p9", str(BLOCKS), str(sector), *ids],
                       stdin=src, stdout=out, check=True)
    return image, ids


def trial(arch, folder):
    sector = 4096
    image, ids = prepare(folder, sector)
    rows = []
    with Terminal(arch, sector, image, "vi-volumes", test=True) as t:
        t.until(b"nsh> ", 900)

        def command(raw, marker):
            start = time.monotonic_ns()
            t.vm.stdin.write(raw.encode())
            result = t.until(marker.encode(), 1800)
            return (time.monotonic_ns()-start)/1e6, result

        for n in SIZES:
            name = f"f{n}"
            print(f"{arch} ouverture {n}", flush=True)
            opening, _ = command(f"vi {name}\n", f"{name} [enregistré] 1:1")
            command("i", f"{name} [enregistré] 1:1")
            insert, _ = command("é", f"{name} [modifié] 1:2")
            remove, _ = command("\x08", f"{name} [enregistré] 1:1")
            command("X\x1b", f"vi [NORMAL] {name} [modifié] 1:1")
            nav, _ = command("G", f"{name} [modifié] {n//128+1}:1")
            command("gg", f"{name} [modifié] 1:1")
            command("x", f"{name} [enregistré] 1:1")
            command("u", f"{name} [modifié] 1:1")
            command("u", f"{name} [enregistré] 1:1")
            command("\x12", f"{name} [modifié] 1:1")
            command(f":w c{n}\r", "Saisir OUI puis Entrée : ")
            command("OUI\r", "fichier enregistré")
            command(":q\r", "nsh> ")
            rows.append(dict(octets=n, ouverture_ms=opening, insertion_ms=insert,
                             suppression_ms=remove, navigation_G_ms=nav))
            print(rows[-1], flush=True)
        t.exit()

    raw = t.archive.read_text()
    pids = re.findall(r"CLI spawn image=14 pid=(\d+)", raw)
    assert len(pids) == len(rows)
    for row, pid in zip(rows, pids):
        samples = [int(a)+int(b) for a,b in re.findall(
            fr"CLI mem pid={pid} privees=(\d+) pile=(\d+)", raw)]
        assert len(samples) >= 3, (pid, samples)
        row["pages_privees_pile_comprise"] = samples
    def host(cmd, *args):
        with image.open("rb") as inp:
            return subprocess.check_output([HOST, cmd, str(BLOCKS), str(sector), *args], stdin=inp)
    host("verify", *ids)
    slots = re.findall(r"slot=(\d+) génération=(\d+)".encode(), host("inspect"))
    slot = max(slots, key=lambda pair: int(pair[1]))[0]
    for n in SIZES:
        for label, expected in ((f"/f{n}", contents(n)), (f"/c{n}", b"X"+contents(n))):
            assert host("salvage", *ids, slot, label) == expected, label
    return rows


if __name__ == "__main__":
    arch = os.environ["NOVA_ARCH"]
    with tempfile.TemporaryDirectory(prefix="nova-vi-volumes-") as temp:
        results = trial(arch, Path(temp))
    (ROOT/"build"/f"vi-volumes-{arch}.json").write_text(json.dumps(results, indent=2)+"\n")
