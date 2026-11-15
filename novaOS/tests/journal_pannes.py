"""Coupures du journal natif sur volumes P9 jetables ; le gestionnaire reste vivant."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal

from journalisation import archive, arret, attend, demarrage, lit_evenement, rows, sortie
from cli import Terminal
from services_configuration import commande, etat
from session_cli import ROOT, fixture_p9


def identite(vm):
    return re.search(r"instance=([a-f0-9/]+)", commande(vm, "@services status 16"))[1]


def essentiel(vm):
    etat(vm, 9, "RUNNING")
    etat(vm, 10, "RUNNING")
    assert "1397118542" in sortie(vm, "service info 9")
    assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")


def notices(vm, marque):
    return [e for e in rows(vm, "journal show self --limit 128")
            if e["code"] == "NOTICE" and e["données"] == f"{marque},0"]


def parcours(arch, vcpu, sector, output):
    output.mkdir(parents=True, exist_ok=False)
    cible = "aarch64-unknown-none-softfloat" if arch == "aarch64" else "x86_64-unknown-none"
    kernel = ROOT / "build/target-journal-pannes" / cible / "release/novaos"
    assert kernel.is_file(), kernel
    noyau = hashlib.sha256(kernel.read_bytes()).hexdigest()
    os.environ["NOVA_KERNEL_ELF"] = str(kernel)
    os.environ["NOVA_VCPUS"] = str(vcpu)
    bilans = []
    for mode in (241, 242, 243, 245):
        image = output / f"{mode}.raw"
        fixture_p9(image, sector)
        log = output / f"{mode}.serial"
        with Terminal(arch, sector, image, f"journal-panne-{mode}", profil_journal=True, journal=log) as vm:
            vm.until(b"nsh> ")
            boot = demarrage(vm)
            etat(vm, 16, "RUNNING")
            avant = identite(vm)
            assert "confirmée" in sortie(vm, "journal emit 177")
            preuve = notices(vm, 177)
            assert len(preuve) == 1
            if mode != 245:
                assert "UNCERTAIN" in sortie(vm, f"journal emit {mode}", 22)
                etat(vm, 16, "RUNNING")
                apres = attend(lambda: identite(vm), lambda i: i != avant and int(i.replace("/", ""), 16) != 0)
                etat(vm, 16, "RUNNING")
                admis = notices(vm, mode)
                assert len(admis) == int(mode == 243), (mode, admis)
                assert notices(vm, 177) == preuve
                for code in ("WITHDRAWN", "RESTART"):
                    lit_evenement(vm, "journal show 16 --limit 128", code=code, source="gestionnaire")
                essentiel(vm)
            else:
                assert "confirmée" in sortie(vm, "journal emit 244")
                sortie(vm, "journal rotate", 26)
                essentiel(vm)
                assert "confirmée" in sortie(vm, "journal emit 245")
                sortie(vm, "journal emit 178", 26)
                sortie(vm, "service stop 10")
                etat(vm, 10, "STOPPED")
                sortie(vm, "service start 10")
                essentiel(vm)
                sortie(vm, "service stop 16")
                etat(vm, 16, "STOPPED")
                sortie(vm, "journal emit 179", 12)
                essentiel(vm)
                sortie(vm, "service start 16")
                etat(vm, 16, "RUNNING")
                apres = identite(vm)
                assert apres != avant
                assert notices(vm, 177) == preuve
                assert notices(vm, 178) == [] and notices(vm, 179) == []
                assert "confirmée" in sortie(vm, "journal emit 180")
                assert len(notices(vm, 180)) == 1
                admis = notices(vm, 245)
                assert len(admis) == 1
            vm.vm.kill()
            assert vm.vm.wait(timeout=10) == -signal.SIGKILL
        # Avant tout nouveau montage, vérifier secteur, P9 et la copie durable.
        snapshot = archive(image, sector, log)
        relance = output / f"{mode}-relance.serial"
        with Terminal(arch, sector, image, f"journal-relit-{mode}", profil_journal=True, journal=relance) as vm:
            vm.until(b"nsh> ")
            assert demarrage(vm) != boot
            historique = rows(vm, f"journal boot --boot {boot} --limit 128")
            def temoins(valeur):
                return [x for x in historique if x["code"] == "NOTICE" and x["service"] == "0" and x["données"] == f"{valeur},0"]
            assert len(temoins(177)) == 1
            assert len(temoins(mode)) == int(mode in (243, 245))
            essentiel(vm)
            arret(vm, vcpu)
        serial = relance.read_text(errors="replace")
        pages = re.findall(r"JOURNAL RAM (lancement|retrait) pid=\d+ privees=(\d+) pile=(\d+) octets_page=4096", serial)
        assert {row[0] for row in pages} == {"lancement", "retrait"}, pages
        assert hashlib.sha256(kernel.read_bytes()).hexdigest() == noyau
        bilans.append(dict(sonde=mode, boot=boot, noyau=noyau, pages_journal=pages,
                            instance_avant=avant, instance_apres=apres,
                            publies=len(admis), coupure=snapshot, remontage=archive(image, sector, relance)))
    (output / "resultats.json").write_text(json.dumps(bilans, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64", "x86_64"))
    p.add_argument("vcpu", type=int, choices=(1, 2, 4))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("preuves", type=Path)
    a = p.parse_args()
    parcours(a.arch, a.vcpu, a.secteur, a.preuves.resolve())
