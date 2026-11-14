"""Session interactive sur UART, ELF isolés et contrôle du disque entre quatre VM."""

import argparse
import json
import os
from pathlib import Path
import re
import select
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from session_cli import HOST, LONG, fixture, machine


class Terminal:
    def __init__(self, arch, sector, image, label, test=False, *, qmp_bloc=None, defaut_bloc=None, entries=None):
        cmd, env = machine(arch, sector, image, test)
        if entries is not None:
            assert entries in (8, 16, 32, 64, 128, 256)
            index, = [i for i, arg in enumerate(cmd) if str(arg).startswith("virtio-blk-pci,id=b,")]
            cmd[index] += f",queue-size={entries}"
        if qmp_bloc is not None or defaut_bloc is not None:
            index = cmd.index("-drive") + 1
            origine = "file=" + str(image.resolve()) + ","
            assert origine in cmd[index]
            config = str(defaut_bloc.resolve()) if defaut_bloc is not None else ""
            cmd[index] = cmd[index].replace(origine, f"file=blkdebug:{config}:{image.resolve()},")
        if qmp_bloc is not None:
            # Ce descripteur privé évite tout serveur QMP sur le réseau.
            cmd += ["-chardev", f"socket,id=banc,fd={qmp_bloc}", "-mon", "chardev=banc,mode=control"]
        self.archive = ROOT / "build" / f"cli-{arch}-{sector}-{label}.serial"
        self.log = self.archive.open("wb")
        self.start = time.monotonic()
        self.vm = subprocess.Popen(
            cmd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, bufsize=0, pass_fds=() if qmp_bloc is None else (qmp_bloc,),
        )
        self.pending = b""

    def __enter__(self):
        return self

    def __exit__(self, *_):
        if self.vm.poll() is None:
            self.vm.kill()
        self.vm.wait()
        self.log.close()

    def until(self, marker, timeout=60, *, refus=None):
        deadline = time.monotonic() + timeout
        while marker not in self.pending:
            rejected = refus is not None and refus in self.pending
            if rejected or b"Panique du noyau" in self.pending or time.monotonic() > deadline:
                raise RuntimeError(f"{self.archive} : attente de {marker!r}, fin {self.pending[-1800:]!r}")
            if not select.select([self.vm.stdout], [], [], 0.2)[0]:
                continue
            chunk = os.read(self.vm.stdout.fileno(), 65536)
            if not chunk:
                raise RuntimeError(f"QEMU arrêté : {self.pending[-800:]!r}")
            self.log.write(chunk)
            self.log.flush()
            self.pending += chunk
        start, _, self.pending = self.pending.partition(marker)
        return start + marker

    def line(self, command, *, status=None, has=(), absent=()):
        raw = command.encode() if isinstance(command, str) else command
        start = time.monotonic_ns()
        self.vm.stdin.write(raw + b"\n")
        # Le rafraîchissement de l'éditeur réimprime parfois l'invite avant Entrée.
        echo = self.until(b"\r\n")
        out = echo + self.until(b"nsh> ")
        if status is not None:
            has = (*has, f"[statut {status}]")
        for expected in has:
            expected = expected.encode() if isinstance(expected, str) else expected
            assert expected in out, (raw, out[-1600:], expected)
        for unexpected in absent:
            assert unexpected not in out, (raw, unexpected)
        return (time.monotonic_ns() - start) / 1e6

    def exit(self):
        self.vm.stdin.write(b"exit\n")
        self.until(b"BLOC session terminee ok=true")


def verify_image(image, sector):
    def cli(action, *args):
        with image.open("rb") as source:
            return subprocess.check_output([HOST, action, "512", str(sector), *args], stdin=source)

    pattern = r"slot=(\d+) génération=(\d+) blocs=\d+ FS=(\w+) LV=(\w+)".encode()
    records = re.findall(pattern, cli("inspect"))
    assert len(records) == 2
    slot, gen, fs, lv = max(records, key=lambda r: int(r[1]))
    cli("verify", fs, lv)
    assert cli("salvage", fs, lv, slot, "/lignes") == LONG
    return int(gen)


def interactive(terminal):
    terminal.until(b"BLOC chaine novaFS=")
    published = time.monotonic()
    terminal.until(b"nsh> ")
    ready = time.monotonic()
    measures = {
        "demarrage_ms": (ready - terminal.start) * 1000,
        "publication_invite_ms": (ready - published) * 1000,
        "commandes_ms": [],
    }

    def run(cmd, **checks):
        timing = terminal.line(cmd, **checks)
        label = cmd if isinstance(cmd, str) else repr(cmd[:48])
        measures["commandes_ms"].append([label, round(timing, 3)])

    run("pwd", status="sortie 0", has=[b"\r\n/\r\n"])
    run("mkdir documents", status="sortie 27", has=["existe déjà"])
    run("cd documents")
    run("pwd", status="sortie 0", has=[b"\r\n/documents\r\n"])
    run("ls", status="sortie 0", has=["fichier exemple.txt"])
    run("cat exemple.txt", status="sortie 0", has=[b"\r\nNovaOS\r\n"])
    run("head -n 1 exemple.txt", status="sortie 0", has=[b"\r\nNovaOS\r\n"])
    run("rm exemple.txt", status="sortie 0")
    run("ls", status="sortie 0", absent=[b"fichier exemple.txt"])

    run("")
    run("absente", has=["commande inconnue"])
    run("./pwd", has=["commande inconnue"])
    run("cat 'inachevé", has=["saisie refusée"])
    run("pwd " + "\\", has=["saisie refusée"])
    run("pwd" + " a" * 33, has=["limite atteinte"])
    run("pwd a b", status="sortie 3", has=["argument invalide"])
    run("head -n -1 /lignes", status="sortie 3")
    run("cd ..")
    run("pwd", status="sortie 0", has=[b"\r\n/\r\n"])
    run("cd ..", has=["accès refusé"])
    run("cd documents")
    run("cd absent", has=["introuvable"])
    run("cat exemple.txt", status="sortie 20", has=["introuvable"])
    run("rm /documents", status="sortie 24", has=["occupé"])

    run("mkdir 'répertoire neuf'", status="sortie 0")
    run('cd "répertoire neuf"')
    run("pwd", status="sortie 0", has=["/documents/répertoire neuf"])
    run("cd /")
    run(r"cd documents/répertoire\ neuf")
    run("pwd", status="sortie 0", has=["/documents/répertoire neuf"])
    run("cd /")
    run("ls -l documents", status="sortie 0", has=["répertoire répertoire neuf", "taille="])

    run("cat /lignes", status="sortie 0", has=[LONG.replace(b"\n", b"\r\n")])
    run("head /lignes", status="sortie 0", has=[b"ligne 9\r\n"], absent=[b"ligne 10\r\n"])
    run("head -n 80 /lignes", status="sortie 0", has=[b"ligne 15\r\nfin"])
    run("cat /controle", status="sortie 0", has=[br"avant\u{1b}[31m\u{0}apres"], absent=[b"\x1b[31m"])
    run("rm -rf /", status="sortie 5", has=["accès refusé"])
    run("rm /p", status="sortie 30", has=["est un répertoire"])

    root = [b"\r\n/\r\n"]
    run("pwé\x7fd", status="sortie 0", has=root)
    run(b"pd\x1b[Dw\x1b[C", status="sortie 0", has=root)
    run(b"pwd\xff", has=["saisie refusée"])
    run(b"pwd\x03", has=["saisie refusée"])
    run(b"z" * 2049, has=["limite atteinte"])
    run("@probe droits", status="sortie 0")
    run("@probe revocation", status="sortie 0")
    run("cd /" + "/".join(["p"] * 32))
    run("mkdir trop-profond", status="sortie 6", has=["limite atteinte"])
    run("cd /")
    run("@probe faute", status="faute 0", has=["processus interrompu"])
    run("pwd", status="sortie 0", has=root)
    for _ in range(4):
        run("pwd", status="sortie 0")
    terminal.exit()
    return measures


def verify_processes(log, arch):
    assert "Panique du noyau" not in log
    spawns = re.findall(r"CLI spawn image=(\d+) pid=(\d+) cycles=(\d+) pages=(\d+)", log)
    assert {int(i) for i, *_ in spawns} == set(range(7))
    level = "EL0" if arch == "aarch64" else "Ring3"
    exits = set(re.findall(rf"PROC pid=(\d+) niveau={level} pc=", log))
    launched = {pid for _, pid, _, _ in spawns}
    collected = set(re.findall(r"CLI collecte pid=(\d+) cycles=", log))
    assert launched <= exits and launched == collected
    idle = re.findall(r"CLI repos pages=(\d+) libres=(\d+) buddy=(\d+)", log)
    assert len(idle) >= 4 and len(set(idle[-4:])) == 1, idle[-5:]
    end = re.findall(r"RAM phase=bloc_collecte .*?ipc=(\d+) utilisateur=(\d+) processus=(\d+)", log)
    assert end == [("0", "0", "0")], end


def check(arch, sector, tmp):
    image = tmp / "volume.raw"
    fixture(image, sector)
    initial = verify_image(image, sector)
    with Terminal(arch, sector, image, "session", test=True) as t:
        measures = interactive(t)
    verify_processes(t.archive.read_text(errors="replace"), arch)
    committed = verify_image(image, sector)
    assert committed > initial

    with Terminal(arch, sector, image, "remontage") as t:
        t.until(b"nsh> ")
        t.line("cd documents")
        t.line("ls", has=["répertoire répertoire neuf"], absent=[b"exemple.txt"])
        t.line("cat exemple.txt", has=["introuvable", "statut 20"])
        t.line("cd 'répertoire neuf'")
        t.line("pwd", has=["/documents/répertoire neuf"])
        t.exit()
    assert verify_image(image, sector) == committed

    with Terminal(arch, sector, image, "coupure") as t:
        t.until(b"nsh> ")
        t.line("mkdir confirme", absent=[b"erreur", b"statut "])
        # La sortie du contexte tue QEMU avant exit/CLOSE.
    after = verify_image(image, sector)
    assert after == committed + 1, (committed, after)
    with Terminal(arch, sector, image, "apres-coupure") as t:
        t.until(b"nsh> ")
        t.line("cd confirme")
        t.line("pwd", has=[b"/confirme\r\n"])
        t.exit()
    assert verify_image(image, sector) == after

    path = ROOT / "build" / f"cli-{arch}-{sector}.json"
    path.write_text(json.dumps(measures, indent=2, ensure_ascii=False) + "\n")
    print(f"OK — {arch}/{sector} : terminal, ELF isolés, révocation, faute, remontages G{committed}/G{after}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sector", type=int, choices=[512, 4096])
    args = parser.parse_args()
    for sector in [args.sector] if args.sector else [512, 4096]:
        with tempfile.TemporaryDirectory(prefix="nova-cli-") as tmp:
            check(os.environ["NOVA_ARCH"], sector, Path(tmp))
