"""Scripts sur de vraies sessions NovaOS et des volumes P9 jetables."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from cli import HOST, ROOT, Terminal

COMMUN = """# variables, choix, sauts, boucles et court-circuit
nom="été blanc"
echo "A:${nom}" '$nom' \\$nom
if false; then echo faux; elif [ "$nom" = "été blanc" ]; then echo oui; else echo faux; fi
n=0
while [ "$n" -lt 2 ]; do
    echo "tour$n"
    if test "$n" -eq 0; then n=1; continue; fi
    n=2
done
for i in a b; do
    for j in 0 1 2; do
        [ "$j" -eq 1 ] && continue
        test "$j" -eq 2 && break
        echo "$i$j"
    done
done
for i in a b; do if test "$i" = b; then break; fi; false; done
echo "break=$?"
for i in a b; do if test "$i" = b; then continue; fi; false; done
echo "continue=$?"
false && echo ignoré || echo secours
! true
echo "statut=$?"
unset nom
test -z "$nom" && echo vide
"""



def fixture(folder, sector):
    # Les quatre cadres atteignent chacun huit structures, avec un véritable ELF au fond.
    deepest = {}
    for i in range(4):
        inner = f"nsh cadre{i + 1}" if i < 3 else "cat garde"
        deepest[f"cadre{i}"] = ("if true; then " * 8 + inner + "; fi" * 8).encode()
    contents = {
        "commun.nsh": COMMUN.encode(),
        "enfant": (
            b"#!/bin/nsh\ncd sous\n"
            b'echo "export=$publique prive=$prive"; publique=autre; return 37; echo interdit\n'
        ),
        "parent": (
            b"#!/bin/nsh\nprive=secret; export publique=visible\n./enfant\n"
            b'echo "retour=$? publique=$publique"; pwd\n'
        ),
        "vide": b"",
        "brise": b"touch interdit; if true; then echo tard\n",
        "utf8-invalide": b"touch interdit;\xff",
        "etranger": b"#!/bin/autre\ntouch interdit\n",
        "boucle": b"nsh boucle\n",
        "grand": b"#" * 8193,
        "stop": b"touch avant; rm -i garde; touch apres\n",
        "garde": b"temoin",
        "note.txt": b"x",
        "sous/a.txt": b"y",
        ".cache.txt": b"z",
        **deepest,
    }
    commands = ["mkdir /sous"]
    for name, data in contents.items():
        commands.append("create /" + name)
        for off in range(0, len(data), 512):
            commands.append(f"write /{name} {off} {data[off:off + 512].hex()}")
    origin, image = folder / "source.raw", folder / "p9.raw"
    with origin.open("xb") as out:
        run = subprocess.run(
            [HOST, "script", "512", str(sector)],
            input=("\n".join(commands) + "\n").encode(), stdout=out, stderr=subprocess.PIPE,
            check=True,
        )
    fs, lv = re.search(rb"FS=(\w+) LV=(\w+)", run.stderr).groups()
    with origin.open("rb") as src, image.open("xb") as out:
        subprocess.run(
            [HOST, "migrate-content-p9", "512", str(sector), fs, lv],
            stdin=src, stdout=out, check=True,
        )
    return image, fs, lv


def trial(arch, sector, folder):
    image, fs, lv = fixture(folder, sector)
    checked = []
    with Terminal(arch, sector, image, "scripts", test=True) as t:
        t.until(b"nsh> ")

        def cmd(line, expected=(), absent=()):
            t.vm.stdin.write(line.encode() + b"\n")
            t.until(b"\r\n")
            output = t.until(b"nsh> ").decode().replace("\r\n", "\n")
            for value in expected:
                assert value in output, (line, value, output)
            for value in absent:
                assert value not in output, (line, value, output)
            checked.append(line)
            return output

        bash = subprocess.check_output(["/bin/bash", "--noprofile", "--norc", "-c", COMMUN], text=True)
        cmd("nsh commun.nsh", [bash])
        cmd("./parent", ["export=visible prive=\n", "retour=37 publique=visible\n", "\n/\n"], ["interdit\n"])
        cmd("nsh vide; echo vide=$?", ["vide=0\n"])
        cmd("nsh -c 'exit 42'; echo code=$?", ["code=42\n"])
        cmd("return 1; echo retour=$?", ["retour=3\n"])
        cmd("break; echo saut=$?", ["saut=3\n"])
        cmd("nsh -c 'false && touch interdit; true || rm garde; ! test -e interdit'; echo rc=$?", ["rc=0\n"])
        cmd("p='**/*.txt'; for i in $p; do echo [$i]; done", ["[note.txt]\n[sous/a.txt]\n"], [".cache.txt"])
        cmd(r'echo "$p" \* "**/*.txt"', ["**/*.txt * **/*.txt\n"])
        pid = cmd("echo $$")
        assert re.search(r"^\d+$", pid, re.M), pid
        t.vm.stdin.write(b"if true; then\n")
        t.until(b"...> ")
        t.vm.stdin.write(b"echo multiligne\n")
        t.until(b"...> ")
        t.vm.stdin.write(b"fi\n")
        assert b"multiligne\r\n" in t.until(b"nsh> ")

        for line in ["nsh brise", "nsh utf8-invalide", "./etranger", "nsh -c 'touch interdit; echo a | cat'"]:
            cmd(line + "; echo refus=$?", ["refus=3\n"])
        cmd("nsh grand", ["limite atteinte"])
        cmd("nsh boucle", ["limite atteinte"])
        cmd("nsh cadre0; echo profond=$?", ["temoin", "profond=0\n"])
        cmd("nsh -c 'while true; do continue; done'", ["limite atteinte"])
        deep = "if true; then " * 9 + "echo interdit" + "; fi" * 9
        cmd("nsh -c '" + deep + "'", ["limite atteinte"], ["interdit\n"])
        many = ";".join(f"v{i}=x" for i in range(33))
        cmd("nsh -c '" + many + "'", ["limite atteinte"])
        cmd("nsh -c 'test -d sous && test -f note.txt && [ aa != bb ] && echo TYPES'", ["TYPES\n"])
        cmd("nsh -c 'echo true; function f { touch interdit; }'", ["argument invalide"], ["true\n"])
        cmd("nsh -c '" + "true;" * 257 + "'", ["limite atteinte"])
        cmd("nsh -c 'x=ab; for i in a b c d e f g h i j k; do x=$x$x; done; echo suite'",
            ["limite atteinte"], ["suite\n"])
        cmd("nsh -c 'inconnue; return'; echo inconnue=$?", ["commande inconnue", "inconnue=127\n"])
        cmd("nsh -c 'for i in; do touch interdit; done; echo boucle=$?'", ["boucle=0\n"])
        cmd("nsh -c '@probe droits'", ["[statut sortie 0]"])
        cmd("nsh -c 'test -e /../garde'; echo statut=$?", ["accès refusé", "statut=5"])
        cmd("nsh -c '@probe revocation'", ["[statut sortie 0]"])
        cmd("nsh -c '@probe faute'; echo faute=$?", ["processus interrompu", "faute=128"])
        cmd("echo bon; test -e interdit; echo absent=$?", ["bon\n", "absent=1\n"])
        t.vm.stdin.write(b"nsh stop\n")
        t.until(b"[o/N] ")

    serial = t.archive.read_text()
    memory = re.findall(r"CLI mem pid=4 privees=(\d+) pile=(\d+)", serial)
    assert memory
    with image.open("rb") as disk:
        subprocess.run([HOST, "verify", "512", str(sector), fs, lv], stdin=disk, check=True)
    with Terminal(arch, sector, image, "scripts-reprise", test=True) as t:
        t.until(b"nsh> ")
        t.line("test -f avant && test -f garde && ! test -e apres && echo RESTAURE",
               has=["\r\nRESTAURE\r\n"])
        t.line("cat garde", status="sortie 0", has=["temoin"])
        t.exit()
    return dict(architecture=arch, secteur=sector, commandes=len(checked), bash=bash.splitlines(),
                pic_pages_privees_observe=max(int(a) + int(b) for a, b in memory), reprise=True)


if __name__ == "__main__":
    arch = os.environ["NOVA_ARCH"]
    for sector in (512, 4096):
        with tempfile.TemporaryDirectory(prefix="nsh-script-") as folder:
            result = trial(arch, sector, Path(folder))
            path = ROOT / "build" / f"scripts-{arch}-{sector}.json"
            path.write_text(json.dumps(result, ensure_ascii=False) + "\n")
            print(result, flush=True)
