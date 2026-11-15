"""Démarrage P9 avec enveloppe déchirée, droit interdit ou deux copies invalides."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
from cli import Terminal
from session_cli import fixture_p9
from services_configuration import commande, etat, generation


def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def trial(arch, sector, folder, tool, cpus=2):
    folder.mkdir(parents=True, exist_ok=False)
    os.environ["NOVA_VCPUS"] = str(cpus)
    source = folder / "initial.raw"
    fixture_p9(source, sector)
    initial = sha(source)
    results = []
    for cas in ("enveloppe", "capacite", "sans-secours"):
        image = folder / f"{cas}.raw"
        subprocess.run([tool, "prepare", source, image, str(sector), cas], check=True)
        before = subprocess.check_output([tool, "inspect", image, str(sector)])
        with image.open("rb") as f, gzip.open(folder / f"{cas}-avant.raw.gz", "wb") as out:
            out.write(f.read())
        pre_sha = sha(image)
        journal = folder / f"{cas}.serial"
        saved_generation = int(cas != "sans-secours")
        with Terminal(arch, sector, image, f"config-invalide-{cas}",
                      journal=journal, profil_config=True) as vm:
            vm.until(b"nsh> ")
            assert "NovaOS" in commande(vm, "cat /documents/exemple.txt")
            status = commande(vm, "service status 9")
            assert "Current state: REGISTERED" in status and "Autostart: disabled" in status, status
            assert generation(status, "Stored") == generation(status, "Effective") == saved_generation, status
            assert "configuration corrompue" in status, status
            if cas == "sans-secours":
                for name in ("clock", "cache"):
                    assert "Autostart: disabled" in commande(vm, f"service status {name}")
            else:
                etat(vm, 1, "RUNNING")
                dep = etat(vm, 10, "WAITING")
                assert "9 indisponible" in dep, dep
            # Le port de maintenance est présent. Une modification qui écraserait
            # les preuves de corruption est refusée même avec ce port.
            refuse = commande(vm, "service enable 9")
            assert "configuration corrompue" in refuse and "enregistré=" not in refuse, refuse
            info = commande(vm, "service info 9")
            assert "service ou transition indisponible" in info and "valeur=" not in info, info
            vm.exit()
            vm.until(b"SMP arret motif=normal")
        after = subprocess.check_output([tool, "inspect", image, str(sector)])
        assert before == after, cas
        results.append(dict(cas=cas, generation=saved_generation,
                            serie=sha(journal), avant=pre_sha, apres=sha(image),
                            racines_et_sentinelle=hashlib.sha256(before).hexdigest()))
    assert sha(source) == initial
    (folder / "resultats.json").write_text(json.dumps(dict(
        arch=arch, secteur=sector, vcpus=cpus, original=initial, outil=sha(tool),
        resultats=results), ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=("aarch64", "x86_64"))
    parser.add_argument("secteur", type=int, choices=(512, 4096))
    parser.add_argument("preuves", type=Path)
    parser.add_argument("outil", type=Path)
    args = parser.parse_args()
    trial(args.arch, args.secteur, args.preuves.resolve(), args.outil.resolve())
