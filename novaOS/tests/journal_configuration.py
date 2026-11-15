"""Une configuration P9 au-delà du plafond installé laisse un diagnostic consultable."""
import argparse
import json
import os
from pathlib import Path
import subprocess

from cli import Terminal
from journalisation import archive, arret, demarrage, lit_evenement, sortie
from services_configuration import etat
from session_cli import fixture_p9


def verifier(arch, secteur, preuves, outil):
    preuves.mkdir(parents=True, exist_ok=False)
    reference, image = preuves / "initial.raw", preuves / "capacite.raw"
    fixture_p9(reference, secteur)
    subprocess.run([outil, "prepare", reference, image, str(secteur), "capacite"], check=True)
    avant = subprocess.check_output([outil, "inspect", image, str(secteur)])
    os.environ["NOVA_VCPUS"] = "2"
    log = preuves / "configuration.serial"
    with Terminal(arch, secteur, image, "journal-config-refus", profil_journal=True, journal=log) as vm:
        vm.until(b"nsh> ")
        boot = demarrage(vm)
        assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
        assert "9 indisponible" in etat(vm, 10, "WAITING")
        events = lit_evenement(vm, "journal boot --level ERROR", code="CONFIG_ERROR", boot=boot)
        assert any(e["catégorie"] == "CONFIG" and e["source"] == "gestionnaire" and
                   e["données"].split(",")[0] == "21" for e in events), events
        arret(vm, 2)
    assert avant == subprocess.check_output([outil, "inspect", image, str(secteur)])
    resultat = dict(boot=boot, erreurs=events, **archive(image, secteur, log))
    (preuves / "resultats.json").write_text(json.dumps(resultat, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64", "x86_64"))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("preuves", type=Path)
    p.add_argument("outil", type=Path)
    a = p.parse_args()
    verifier(a.arch, a.secteur, a.preuves.resolve(), a.outil.resolve())
