"""Chronométrer vi à partir de l'UART de QEMU. Les sondes internes sont facultatives."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile
import time

from cli import Terminal
import vi_volumes as volumes


def ms(start, end):
    return round((end - start) / 1e6, 3)


def run(arch, sizes, directory, probes=False, read_only=False, revisit=False):
    sector = 4096
    if any(n <= 0 or n % len(volumes.LINE) for n in sizes) or len(set(sizes)) != len(sizes):
        raise ValueError("Tailles positives distinctes, multiples de 128 octets")
    volumes.SIZES = sizes
    volumes.BLOCKS = max(4096, (2 if read_only else 4) * sum(sizes) // 4096)
    image, ids = volumes.prepare(directory, sector)
    rows = []
    label = ("vi-perf-ro-" if read_only else "vi-perf-") + str(sizes[0])
    if revisit:
        label += "-index"
    with Terminal(arch, sector, image, label, test=True) as term:
        term.until(b"nsh> ", 900, refus=b"BLOC session terminee ok=false")

        def send(data):
            term.vm.stdin.write(data.encode())

        def until(marker):
            term.until(marker.encode(), 3600, refus=b"BLOC session terminee ok=false")
            return time.monotonic_ns()

        for n in sizes:
            name = f"f{n}"
            start = time.monotonic_ns()
            send(f"vi {name}\n")
            spawned = until("CLI spawn image=14 pid=")
            stages = {}
            if probes:
                begin = until("VI-PROF vi-entree")
                received = until("VI-PROF vi-recu")
                prepared = until("VI-PROF vi-prepare")
                stages = dict(transfert_sonde_ms=ms(begin, received),
                              preparation_sonde_ms=ms(received, prepared))
            query = until("\x1b[6n")
            send("\x1b[24;80R")
            shown = until(f"{name} [enregistré] 1:1")
            send("G")
            navigated = until(f"{name} [enregistré] {n//128+1}:1")
            if revisit:
                send("gg")
                home=until(f"{name} [enregistré] 1:1")
                send("G")
                again=until(f"{name} [enregistré] {n//128+1}:1")
                stages["retour_G_ms"]=ms(home,again)
            if read_only:
                send(":q\r")
                until("nsh> ")
                rows.append(dict(octets=n, lecture_et_lancement_ms=ms(start, spawned),
                                 transfert_et_preparation_ms=ms(spawned, query), **stages,
                                 premier_rendu_ms=ms(query, shown), ouverture_ms=ms(start, shown),
                                 navigation_G_ms=ms(shown, navigated)))
                continue
            send("gg")
            until(f"{name} [enregistré] 1:1")
            send("iX\x1b")
            until(f"vi [NORMAL] {name} [modifié] 1:1")
            saving = time.monotonic_ns()
            send(f":w c{n}\r")
            confirm = until("Saisir OUI puis Entrée : ")
            send("OUI\r")
            saved = until("fichier enregistré")
            send(":q\r")
            until("nsh> ")
            rows.append(dict(octets=n, lecture_et_lancement_ms=ms(start, spawned),
                             transfert_et_preparation_ms=ms(spawned, query), **stages,
                             premier_rendu_ms=ms(query, shown), ouverture_ms=ms(start, shown),
                             navigation_G_ms=ms(shown, navigated),
                             transfert_avant_oui_ms=ms(saving, confirm),
                             publication_apres_oui_ms=ms(confirm, saved)))
            print(json.dumps(rows[-1]), flush=True)
        term.exit()

    raw = term.archive.read_bytes()
    pids = re.findall(rb"CLI spawn image=14 pid=(\d+)", raw)
    assert len(pids) == len(rows)
    for row, pid in zip(rows, pids):
        samples = re.findall(rb"CLI mem pid=" + pid + rb" privees=(\d+) pile=(\d+)", raw)
        assert len(samples) >= (2 if read_only else 3)
        row["resident_vi_kio"] = [4 * (int(p) + int(s)) for p, s in samples]

    def read(op, *args):
        with image.open("rb") as stream:
            return subprocess.check_output(
                [volumes.HOST, op, str(volumes.BLOCKS), str(sector), *args], stdin=stream)

    read("verify", *ids)
    slots = re.findall(r"slot=(\d+) génération=(\d+)".encode(), read("inspect"))
    slot = max(slots, key=lambda pair: int(pair[1]))[0]
    for n in sizes:
        original = volumes.contents(n)
        assert read("salvage", *ids, slot, f"/f{n}") == original
        if read_only:
            continue
        assert read("salvage", *ids, slot, f"/c{n}") == b"X" + original
    return rows


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=["aarch64", "x86_64"])
    p.add_argument("rapport", type=Path)
    p.add_argument("sizes", type=int, nargs="+")
    p.add_argument("--sondes", action="store_true", help="image isolée avec vi-profil.patch")
    p.add_argument("--lecture-seule", action="store_true", help="ouvrir, aller à la fin et quitter sans sauvegarde")
    p.add_argument("--retour-fin",action="store_true",help="répéter G après gg pour chronométrer l’index")
    args = p.parse_args()
    with tempfile.TemporaryDirectory(prefix="nova-vi-perf-") as tmp:
        result = run(args.arch, tuple(args.sizes), Path(tmp), args.sondes, args.lecture_seule,args.retour_fin)
    args.rapport.write_text(json.dumps(result, indent=2) + "\n")
