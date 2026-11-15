"""Retrait du droit réellement délégué par 2, et du droit de fournisseur de 10."""
import argparse
import json
import os
from pathlib import Path

from cli import Terminal
from journalisation import archive, arret, demarrage, sortie
from services_capacites import changer, droits, etat, journal, memoire, verifie
from session_cli import fixture_p9


def qualifier(arch, sector, folder):
    folder.mkdir(parents=True, exist_ok=False)
    os.environ["NOVA_VCPUS"] = "2"
    image = folder / "services.raw"
    fixture_p9(image, sector)
    serial = folder / "delegation.serial"
    events = {}
    with Terminal(arch, sector, image, "cap-delegation", profil_journal=True, journal=serial) as vm:
        vm.until(b"nsh> ")
        boot = demarrage(vm)
        for id_, cap in ((2, "private-tmp"), (10, "cache-backend")):
            etat(vm, id_, "RUNNING")
            changer(vm, "restrict", id_, cap)
            verifie(droits(vm, id_), cap, "aucune", cap, "yes")
            sortie(vm, f"service restart {id_}")
            etat(vm, id_, "FAILED")
            # CAP_MISSING implique la sortie effective DENIED de l'ELF en STARTING.
            # Pour 2, ce retour suit l'enfant sans héritage et le refus de TMP_WORKER.
            events[id_] = journal(vm, id_, "CAP_MISSING", boot)
            verifie(droits(vm, id_), cap, "aucune", "aucune instance active")
            assert "1397118542" in sortie(vm, "service info 9")
            changer(vm, "restore", id_, cap)
            sortie(vm, f"service start {id_}")
            etat(vm, id_, "RUNNING")
            verifie(droits(vm, id_), cap, cap, cap, "no")
        arret(vm, 2)
    result = dict(architecture=arch, secteur=sector, vcpu=2, boot=boot, refus=events,
                  memoire=memoire(serial), **archive(image, sector, serial))
    (folder / "resultats.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64", "x86_64"))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("preuves", type=Path)
    args = p.parse_args()
    qualifier(args.arch, args.secteur, args.preuves.resolve())
