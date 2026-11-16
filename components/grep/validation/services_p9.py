"""Recette des interfaces de fichiers employées par les commandes sur un P9 neuf."""
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "novaOS/tests"))
from cli import Terminal
from journalisation import sortie
from nsh_scripts import COMMUN, fixture


def main(arch, sector, out):
    os.environ.update(NOVA_VCPUS="2", NOVA_RAM="256")
    out.mkdir(parents=True, exist_ok=False)
    disk, _, _ = fixture(out, sector)
    before = hashlib.sha256(disk.read_bytes()).hexdigest()
    names = ("sous", "commun.nsh", "enfant", "parent", "vide", "brise",
             "utf8-invalide", "etranger", "boucle", "grand", "stop", "garde",
             "note.txt", ".cache.txt", "cadre0", "cadre1", "cadre2", "cadre3")
    results = {}
    with Terminal(arch, sector, disk, "services-grep", journal=out / "services.serial") as vm:
        vm.until(b"nsh> ")
        result = sortie(vm, "ls -latr --color /")
        for name in names:
            assert name in result, (name, result)
        assert "\x1b[34m" in result, result
        results["ls -latr --color /"] = list(names)
        # The fixture contains 8193 bytes and no final newline.
        vm.vm.stdin.write(b"nsh -c 'echo debutlecture; cat grand; echo finlecture=$?'\n")
        vm.until(b"\r\ndebutlecture\r\n")
        result = vm.until(b"finlecture=")
        assert vm.until(b"\r\n").strip() == b"0"
        vm.until(b"nsh> ")
        assert result.count(b"#") == 8193, result[-400:]
        results["cat grand"] = {"octets_attendus_et_lus": 8193, "octet": "23"}
        result = sortie(vm, "cat commun.nsh | wc -l")
        count = COMMUN.count("\n")
        assert str(count) in result.splitlines(), result
        results["cat commun.nsh | wc -l"] = count
    after = hashlib.sha256(disk.read_bytes()).hexdigest()
    (out / "resultats.json").write_text(json.dumps(dict(
        architecture=arch, secteur=sector, avant=before, apres=after, resultats=results
    ), ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main(sys.argv[1], int(sys.argv[2]), Path(sys.argv[3]).resolve())
