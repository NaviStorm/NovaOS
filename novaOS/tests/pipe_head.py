"""Deux ELF natifs sur un volume P9 isolé. Conserve ISO, disque, UART et mesures."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

from cli import Terminal
from journalisation import arret
from pipe_natif import fixture, fingerprint, host
from preuves_stockage import figer_initiale

FIRST = b"premiere\nSECONDE\n"
FILES = {
    "grand": FIRST + (b"0123456789abcdef" * 4096)[:65536-len(FIRST)],
    "court": b"A\nSECONDE\n",
    "vide": b"",
    "sanslf": b"fragment-sans-LF",
    "initial": b"\nSECONDE",
    "fintrame": b"x"*127 + b"\nSECONDE",
    "debuttrame": b"y"*128 + b"\nSECONDE",
    "longue": b"abcdefgh" * 625 + b"\nSECONDE",
    "utf": b"u"*127 + "€".encode() + b"\x00\x1b\xff\nSECONDE",
    "tronque": b"z"*127 + b"\xe2\x82",
    "voisin": b"voisin-intact\n",
}


def rendered(raw):
    result = []
    for char in raw.decode("utf-8", "surrogateescape"):
        n = ord(char)
        if 0xdc80 <= n <= 0xdcff:
            result.append(f"\\x{n-0xdc00:02x}")
        elif n < 32 and char != "\n":
            result.append(f"\\u{{{n:x}}}")
        else:
            result.append(char)
    return "".join(result).encode()


def drained(path):
    row = re.search(rb"^RAM phase=bloc_collecte .*", path.read_bytes(), re.M).group()
    fields = dict(re.findall(rb"\b(ipc|utilisateur|processus|dma|libres)=(\d+)", row))
    assert len(fields) == 5 and all(int(fields[k]) == 0 for k in (b"ipc", b"utilisateur", b"processus"))
    return {k.decode(): int(v) for k, v in fields.items()}


def qualify(arch, sector, out):
    os.environ.update(NOVA_VCPUS="2", NOVA_RAM="256")
    out.mkdir(parents=True, exist_ok=False)
    ghead = shutil.which("ghead") or "head"
    version = subprocess.check_output([ghead, "--version"], text=True).splitlines()[0]
    assert "GNU coreutils" in version
    expected = {k: subprocess.run([ghead, "-n", "1"], input=v, capture_output=True, check=True).stdout
                for k, v in FILES.items()}
    image, fs, lv = fixture(out, sector, FILES)
    figer_initiale(image)
    observations = []
    report = {"architecture": arch, "secteur": sector, "oracle": version, "essais": observations}

    def save():
        (out/"resultats.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")

    def run(vm, command, rc=0, gate=None):
        vm.vm.stdin.write(f"{command}; echo FLUX_RC_$?__fin\n".encode())
        raw = vm.until(b"\r\n")  # Écho de la commande du terminal.
        if gate is not None:
            raw += vm.until(gate)
            vm.vm.stdin.write(b"+")
        raw += vm.until(f"FLUX_RC_{rc}__fin".encode())
        vm.until(b"nsh> ")
        result = raw.replace(b"\r\n", b"\n")
        observations.append(dict(session=vm.archive.name, commande=command, statut=rc,
                                 extrait_sha256=hashlib.sha256(raw).hexdigest()))
        save()
        return result

    def pair(serial, writer, reader):
        roles = re.findall(rb"PIPE role=(cat|head) pid=(\d+) isole=true", serial)
        assert [r for r, _ in roles] == [b"cat", b"head"] and roles[0][1] != roles[1][1], serial
        assert sorted(re.findall(rb"CLI collecte pid=(\d+)", serial)) == sorted(p for _, p in roles)
        assert b"PIPE admission image=3 fichiers=0 terminal=" in serial
        assert f"[pipe cat={writer}]".encode() in serial and f"[pipe head={reader}]".encode() in serial
        assert b"PIPE retrait noeuds=0 attentes=0 file=0" in serial
        observations[-1].update(pid_cat=int(roles[0][1]), pid_head=int(roles[1][1]), cat=writer, head=reader)

    control = out/"temoin.serial"
    with Terminal(arch, sector, image, "pipe-head", test=True, journal=control) as vm:
        vm.until(b"nsh> ")
        run(vm, "pwd")
        arret(vm, 2)
    report["ressources_temoin"] = drained(control)

    with Terminal(arch, sector, image, "pipe-head", journal=out/"ordinaire.serial") as vm:
        vm.until(b"nsh> ")
        for name in FILES:
            s = run(vm, f"cat {name} | head -n 1")
            body = s.split(b"\n", 1)[1]
            visible = rendered(expected[name])
            # L'UART multiplexe la sortie et les diagnostics d'entretien du tmp.
            # Ces motifs exacts n'apparaissent dans aucun des fichiers témoins.
            payload = body.removesuffix(b"FLUX_RC_0__fin")
            payload = re.sub(rb"DEBUG pid=\d+ TMP retrait confirme\n|TMP retrait instance=\d+ statut=0\n", b"", payload)
            payload = payload.replace("cat : canal fermé\n".encode(), b"")
            assert payload == visible, (name, payload[-800:], visible[-200:])
            observations[-1].update(octets_logiques=len(expected[name]),
                                    oracle_hex=expected[name].hex(), rendu_hex=visible.hex())
        assert rendered(expected["utf"]) in run(vm, "head -n 1 utf")
        assert b"premiere\n" in run(vm, "nsh -c 'cat grand|head -n 1'")
        assert b"\n2\n" in run(vm, "cat court | wc -l")
        run(vm, "cat grand | head -n 2", 3)
        run(vm, "nsh -c 'touch interdit; cat grand | echo a'", 3)
        run(vm, "test -e interdit", 1)
        arret(vm, 2)

    log = out/"qualification.serial"
    with Terminal(arch, sector, image, "pipe-head", test=True, journal=log) as vm:
        vm.until(b"nsh> ")
        for attempt in range(3):
            s = run(vm, "@probe pipe-head-pression grand", gate=b"PIPE attente producteur")
            pair(s, 11, 0)
            markers = [b"PIPE attente producteur depots=8 octets=1024", b"PIPE role=head",
                       b"PIPE premiere reception octets=128",
                       b"PIPE fermeture lecteur lus=128 depots=7", b"PIPE reveil producteur",
                       b"statut=11", b"[pipe cat=11]"]
            positions = [s.index(marker) for marker in markers]
            assert b"PIPE admission image=3 fichiers=0 terminal=6" in s
            assert b"PIPE admission image=2 fichiers=1 terminal=4" in s
            assert positions == sorted(positions) and b"pic=8 blocages=1" in s, s
            assert b"premiere\n" in s and b"SECONDE" not in s
            assert f"lus=128 fnv={fingerprint(FILES['grand'][:128])}".encode() in s
        s = run(vm, "cat grand | head -n 1")
        pair(s, 11, 0)
        s = run(vm, "@probe pipe-head-acquitte court", gate=b"PIPE fermeture producteur")
        pair(s, 0, 0)
        assert s.index(b"PIPE fermeture producteur") < s.index(b"PIPE role=head")
        assert b"pic=1 blocages=0" in s and b"SECONDE" not in s
        for name in ("vide", "sanslf", "tronque"):
            s = run(vm, f"cat {name} | head -n 1")
            pair(s, 0, 0)
            assert f"lus={len(FILES[name])} fnv={fingerprint(FILES[name])}".encode() in s
        for name in ("court", "vide"):
            s = run(vm, f"@probe pipe-head-lecteur {name}", gate=b"PIPE attente lecteur")
            pair(s, 0, 0)
            next_event = b"PIPE premiere reception" if name == "court" else b"PIPE fermeture producteur"
            assert s.index(b"PIPE attente lecteur") < s.index(next_event)
        s = run(vm, "@probe pipe-head-retrait-lecteur court", 10, gate=b"PIPE attente lecteur")
        pair(s, 5, 10)
        assert b"lus=0 fnv=" in s
        s = run(vm, "cat inexistant | head -n 1")
        pair(s, 20, 0)
        assert b"introuvable" in s and b"lus=0 fnv=" in s
        s = run(vm, "@probe pipe-head-admission grand", 5, gate=b"PIPE attente producteur")
        assert b"[pipe cat=10]" in s and b"PIPE role=head" not in s
        assert b"PIPE retrait noeuds=0 attentes=0 file=0" in s
        s = run(vm, "@probe pipe-head-revocation grand", 5, gate=b"PIPE attente producteur")
        pair(s, 10, 5)
        assert b"lus=0 fnv=" in s and b"PIPE premiere reception" not in s
        s = run(vm, "@probe pipe-head-sortie grand", 5, gate=b"PIPE attente producteur")
        pair(s, 11, 5)
        assert b"premiere\n" not in s
        for lang, title in (("fr-CA", "Afficher"), ("en-US", "Display")):
            run(vm, f"export LANG={lang}")
            s = run(vm, "cat grand | head --help")
            assert title.encode() in s and b"PIPE role=" not in s and b"CLI spawn" not in s
            s = run(vm, "cat grand | head --version")
            assert b"head (nova-cli)" in s and b"PIPE role=" not in s and b"CLI spawn" not in s
        assert b"SESSION-OK\n" in run(vm, "echo SESSION-OK")
        arret(vm, 2)
    report["ressources_finales"] = drained(log)
    assert report["ressources_temoin"] == report["ressources_finales"], report["ressources_finales"]

    memory = out/"longue.serial"
    with Terminal(arch, sector, image, "pipe-head", test=True, journal=memory) as vm:
        vm.until(b"nsh> ")
        measured = run(vm, "cat longue | head -n 1")
        assert b"PIPE role=head" in measured and b"[pipe head=0]" in measured
        arret(vm, 2)
    assert drained(memory) == report["ressources_temoin"]
    for key, path in (("pic_ligne_longue", memory), ("pic_temoin", control)):
        report[key] = int(re.search(rb"BLOC memoire .*?pic_cadres=(\d+)", path.read_bytes()).group(1))

    records = host(image, "inspect", "512", str(sector))
    slot, _ = max(re.findall(r"slot=(\d+) génération=(\d+)".encode(), records), key=lambda v: int(v[1]))
    host(image, "verify", "512", str(sector), fs, lv)
    for name, data in FILES.items():
        assert host(image, "salvage", "512", str(sector), fs, lv, slot, "/"+name) == data, name
    report["fichiers_relus_sha256"] = {k: hashlib.sha256(v).hexdigest() for k, v in FILES.items()}
    save()
    print(f"{arch} P9/{sector}: {len(observations)} commandes, {len(FILES)} fichiers relus")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("architecture", choices=("aarch64", "x86_64"))
    parser.add_argument("secteur", type=int, choices=(512, 4096))
    parser.add_argument("preuves", type=Path)
    args = parser.parse_args()
    qualify(args.architecture, args.secteur, args.preuves.resolve())
