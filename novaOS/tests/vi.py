"""Éditeur natif dans un invité, avec relecture indépendante des octets du volume P9."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from cli import HOST, ROOT, Terminal

FRONTIERE = b"x" * 4095 + "猫".encode()
SOURCE = "premier\r\nété\nfin".encode()


def fixture(folder, sector):
    files = {"source": SOURCE, "vide": b"", "long": b"L" * 8192,
             "grand": b"A" * 8193, "lignes": b"\n" * 512, "invalide": b"A\xff",
             "frontiere": FRONTIERE, "tronque": FRONTIERE[:-1], "nul": b"a\0b",
             "controle": b"\x1b[2J", "occupe": b"protege"}
    # La taille logique du fichier creux dépasse la RAM de l'invité (256 Mio).
    commands = ["mkdir /d", "create /penurie", "write /penurie 536870911 5a"]
    for name, data in files.items():
        commands.append(f"create /{name}")
        for at in range(0, len(data), 512):
            commands.append(f"write /{name} {at} {data[at:at + 512].hex()}")
    image = folder / "volume.raw"
    with (folder / "base.raw").open("x+b") as raw, image.open("xb") as out:
        boot = subprocess.run([HOST, "script", "1024", str(sector)], input=("\n".join(commands)+"\n").encode(),
                              stdout=raw, stderr=subprocess.PIPE, check=True)
        fs, lv = re.search(rb"FS=(\w+) LV=(\w+)", boot.stderr).groups()
        raw.seek(0)
        subprocess.run([HOST, "migrate-content-p9", "1024", str(sector), fs, lv], stdin=raw,
                       stdout=out, check=True)
    return image, fs, lv


def trial(arch, sector, folder):
    image, fs, lv = fixture(folder, sector)
    script = "#!/bin/nsh\n\necho 'vi persistant été'\n"
    with Terminal(arch, sector, image, "vi", test=True) as t:
        t.until(b"nsh> ")

        def send(s):
            t.vm.stdin.write(s.encode() if isinstance(s, str) else s)

        def wait(s):
            return t.until(s.encode())

        def open_vi(command):
            send(command+"\n")
            wait("vi [NORMAL]")

        def leave(command):
            send(command+"\r")
            wait("[statut sortie 0]")
            wait("nsh> ")

        t.line("vi --help", has=["vi [--] fichier"])
        t.line("vi /../secret", has=["accès refusé"])
        t.line("vi penurie", has=["contrôle interdit"])
        t.line("ls -l penurie", has=["536870912"])
        t.line("vi invalide", has=["argument invalide"])
        t.line("vi tronque", has=["argument invalide"])
        t.line("vi nul", has=["contrôle interdit"])
        open_vi("vi frontiere")
        send("$")
        wait("1:4096")
        leave(":q")
        open_vi("vi grand")
        send("$")
        wait("1:8193")
        leave(":q")
        open_vi("vi lignes")
        send("G")
        wait("513:1")
        leave(":q")
        t.line("vi controle", has=["contrôle interdit"])
        t.line("vi d", has=["est un répertoire"])

        open_vi("vi d/script.nsh")
        send("i"+script.replace("\n\n", "\r\n\n", 1)+"\x1b:q\r")
        wait("modifications non sauvegardées")
        send(":inconnue\r")
        wait("commande non prise en charge")
        leave(":wq")
        t.line("nsh d/script.nsh", has=["vi persistant été"])

        open_vi("vi source")
        send("G0ggjl$x")
        send("u")  # restaure le é final
        send("/été\rn")
        wait("trouvé")
        send("0aX\x08Y\x1b:w\r")
        wait("remplacement refusé")
        send(":w occupe\r")
        wait("remplacement refusé")
        send(":w /../intrus\r")
        wait("destination non autorisée")
        send(":w d/copie\r")
        wait("Saisir OUI puis Entrée : ")
        send("NON\r")
        wait("destination non autorisée")
        send(":w d/copie\r\n")
        wait("Saisir OUI puis Entrée : ")
        send("OUI\r\n")
        wait("fichier enregistré")
        send("iX\x1b:wq\r")
        wait("remplacement refusé")
        send(":q\r")
        wait("modifications non sauvegardées")
        send("u")
        leave(":x")

        open_vi("vi vide")
        send("Ohaut\x1bGoqueue\x1bggdd")
        send("u")
        leave(":q!")

        open_vi("vi touches")
        send("iAB\x1b[DZ\x1b")  # insertion entre A et B
        send("oété\x1bgg$hxlG0a!\x1bggOhaut\x1b")
        send("Gdd")
        leave(":wq")
        open_vi("vi long")
        send("$")
        wait("1:8192")
        send("aX")
        wait("1:8194")
        send("\x1b:w d/long-copie\r")
        wait("Saisir OUI puis Entrée : ")
        send("OUI\r")
        wait("fichier enregistré")
        leave(":x")
        open_vi("vi fleches")
        send(b"i12\r34\x1b[A\x1bODX\x1b[B\x1b[CZ\xff")
        wait("caractère refusé")
        send("\x1b")
        leave(":wq")
        open_vi("vi historique")
        send("iété\x1ba!\x1bxxuuuu")
        wait("historique [nouveau] 1:1")
        send("\x12\x12\x12\x12uu")
        send("aZ\x1b\x12")
        wait("aucun rétablissement")
        leave(":wq")
        open_vi("vi neuf")
        leave(":x")
        open_vi("@probe vi-revoque")
        send("irevu\x1b:w\r")
        # Le noyau invalide l'alias réduit et le destinataire du canal.
        wait("capacité révoquée")
        send(":w d/repris\r")
        wait("Saisir OUI puis Entrée : ")
        send("OUI\r")
        wait("fichier enregistré")
        leave(":q")
        open_vi("@probe vi-concurrence")
        send("iprive\x1b:w\r")
        wait("remplacement refusé")
        leave(":q!")
        t.exit()

    log = t.archive.read_text()
    pids = set(re.findall(r"CLI spawn image=14 pid=(\d+)", log))
    samples = [int(a)+int(b) for p, a, b in re.findall(r"CLI mem pid=(\d+) privees=(\d+) pile=(\d+)", log) if p in pids]
    assert pids and samples
    assert all("CLI collecte pid="+p+" " in log for p in pids)

    def host(cmd, *args):
        with image.open("rb") as raw:
            return subprocess.check_output([HOST, cmd, "1024", str(sector), *args], stdin=raw)

    expected = {"/frontiere": FRONTIERE, "/tronque": FRONTIERE[:-1], "/nul": b"a\0b",
                "/source": SOURCE, "/d/copie": "premier\r\néYté\nfin".encode(),
                "/occupe": b"protege", "/vide": b"", "/long": b"L"*8192,
                "/d/long-copie": b"L"*8192+b"X", "/neuf": b"",
                "/d/script.nsh": script.encode(), "/vi-essai.nsh": b"tiers\n",
                "/d/repris": b"revu", "/historique": "été!Z".encode(), "/touches": b"haut\nAB", "/fleches": b"1X2\n34Z"}
    def verify_bytes():
        host("verify", fs, lv)
        slots = re.findall(r"slot=(\d+) génération=(\d+)".encode(), host("inspect"))
        slot, _ = max(slots, key=lambda e: int(e[1]))
        for path, content in expected.items():
            actual = host("salvage", fs, lv, slot, path)
            assert actual == content, (path, actual[:160], content[:160])

    verify_bytes()
    with Terminal(arch, sector, image, "vi-reprise") as t:
        t.until(b"nsh> ")
        t.line("./d/script.nsh", has=["\r\nvi persistant été\r\n"])
        t.exit()
    verify_bytes()
    return dict(architecture=arch, secteur=sector, elf_collectes=len(pids),
                fichiers_relus=len(expected), pic_pages_privees_observe=max(samples),
                refus_concurrent=True, revocation=True, refus_source_creuse_binaire_octets=536870912,
                script_apres_redemarrage=True, octets_apres_redemarrage=True)


if __name__ == "__main__":
    for sector in (512, 4096):
        with tempfile.TemporaryDirectory(prefix="nova-vi-") as tmp:
            result = trial(os.environ["NOVA_ARCH"], sector, Path(tmp))
            (ROOT/"build"/f"vi-{result['architecture']}-{sector}.json").write_text(json.dumps(result)+"\n")
            print(result, flush=True)
