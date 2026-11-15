"""Éprouve le seuil d'inauguration du journal sur un P9 réellement rempli dans QEMU."""
import argparse
from pathlib import Path
import hashlib
import json
import os
import subprocess

from cli import Terminal
from journalisation import arret, archive, sortie
from services_configuration import etat, incarnation
from session_cli import fixture_p9


def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def trial(arch, sector, cpus, folder, tool, kernel):
    folder.mkdir(parents=True, exist_ok=False)
    os.environ["NOVA_VCPUS"] = str(cpus)
    os.environ["NOVA_KERNEL_ELF"] = str(kernel)
    source = folder / "original.raw"
    image = folder / "plein.raw"
    serie = folder / "plein.serial"
    fixture_p9(source, sector)
    original = sha(source)
    before = json.loads(subprocess.check_output([tool, "prepare", source, image, str(sector), "55"]))
    assert 16 <= before["blocs_ordinaires_libres"] < 80
    assert before["octets_sentinelle"] >= 128 * 1024
    initial_image = sha(image)
    probe = folder / "sonde-enospc.raw"
    assert subprocess.check_output([tool, "probe", image, probe, str(sector)]).strip() == b"NoSpace"
    assert sha(probe) == initial_image
    with Terminal(arch, sector, image, "journal-saturation-reserve", journal=serie, profil_journal=True) as vm:
        vm.until(b"nsh> ")
        assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
        etat(vm, 9, "RUNNING")
        etat(vm, 10, "RUNNING")
        assert "1397118542" in sortie(vm, "service info 9")
        etat(vm, 16, "FAILED")
        unavailable = sortie(vm, "journal status", (5, 10, 11, 12))
        assert "révision=" not in unavailable
        instance_a = incarnation(vm)
        sortie(vm, "service stop 9")
        etat(vm, 9, "STOPPED")
        sortie(vm, "service start 9")
        etat(vm, 9, "RUNNING")
        instance_b = incarnation(vm)
        assert instance_b != instance_a
        assert "1397118542" in sortie(vm, "service info 9")
        assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
        arret(vm, cpus)
    after = json.loads(subprocess.check_output([tool, "inspect", image, str(sector)]))
    assert 0 < after["blocs_ordinaires_libres"] < 80
    for k in ("octets_sentinelle", "oid_sentinelle", "oid_exemple", "revision_journal", "prochaine_sequence"):
        assert after[k] == before[k], (k, before, after)
    assert after["revision_journal"] == 0 and after["prochaine_sequence"] == 1
    assert sha(source) == original
    report = dict(architecture=arch, secteur=sector, vcpus=cpus, noyau=sha(kernel), outil=sha(tool),
                  original=original, avant_sha256=initial_image, sonde_enospc=sha(probe), avant=before, apres=after,
                  ancienne_instance=instance_a, nouvelle_instance=instance_b, journal_indisponible=unavailable,
                  **archive(image, sector, serie))
    (folder / "resultats.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64", "x86_64"))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("preuves", type=Path)
    p.add_argument("outil", type=Path)
    p.add_argument("noyau", type=Path)
    p.add_argument("--vcpu", type=int, choices=(1, 2, 4), default=2)
    a = p.parse_args()
    trial(a.arch, a.secteur, a.vcpu, a.preuves.resolve(), a.outil.resolve(), a.noyau.resolve())
