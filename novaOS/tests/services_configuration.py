"""Persistance du service d’information de stockage et de son dépendant sur un même disque P9."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import time

from cli import Terminal, verify_image
from session_cli import fixture_p9
from services_configuration_nsh import verifie_console_ordinaire


def commande(vm, ligne):
    vm.vm.stdin.write(ligne.encode() + b"\n")
    vm.until(b"\r\n")
    return vm.until(b"nsh> ").decode("utf-8", errors="replace")


def etat(vm, identifiant, attendu):
    fin = time.monotonic() + 30
    while True:
        texte = commande(vm, f"service status {identifiant}")
        if f"Current state: {attendu}" in texte:
            return texte
        if time.monotonic() >= fin:
            raise AssertionError((identifiant, attendu, texte))
        time.sleep(0.05)


def incarnation(vm):
    texte = commande(vm, "@services status 9")
    return re.search(r"instance=([a-f0-9/]+)", texte)[1]


def generation(texte, champ):
    return int(re.search(rf"{champ} generation: (\d+)", texte)[1])


def confirme(texte, marque):
    assert marque in texte, (marque, texte)


def cellule(arch, vcpus, secteur, dossier):
    dossier.mkdir(parents=True, exist_ok=False)
    image = dossier / "volume.raw"
    fixture_p9(image, secteur)
    os.environ["NOVA_VCPUS"] = str(vcpus)
    bilans = []
    for numero, profil in enumerate(("ordinaire", "desactiver", "reactiver", "restaure")):
        admin = profil != "ordinaire"
        serie = dossier / f"{numero}-{profil}.serial"
        with Terminal(arch, secteur, image, f"config-{vcpus}-{profil}", journal=serie,
                      profil_config=admin) as vm:
            vm.until(b"nsh> ")
            confirme(commande(vm, "cat /documents/exemple.txt"), "NovaOS")
            if profil == "ordinaire":
                etat(vm, 9, "RUNNING")
                verifie_console_ordinaire(vm, commande)
                confirme(commande(vm, "service info 9"), "1397118542")
                reponse = commande(vm, "service disable 9")
                assert "enregistré=disabled" not in reponse and "refus" in reponse.lower(), reponse
            elif profil == "desactiver":
                avant = etat(vm, 9, "RUNNING")
                initiale = generation(avant, "Effective")
                etat(vm, 10, "RUNNING")
                confirme(commande(vm, "service dependencies 10"), "id=9")
                confirme(commande(vm, "service disable 9"), "enregistré=disabled")
                apres = etat(vm, 9, "RUNNING")
                conservee = generation(apres, "Stored")
                assert conservee > initiale and generation(apres, "Effective") == initiale
                confirme(apres, "Autostart: disabled")
                confirme(apres, "Pending restart: yes")
                cache = etat(vm, 10, "RUNNING")
                confirme(cache, "Autostart: enabled")
                confirme(cache, "Pending restart: no")
                confirme(commande(vm, "service info 9"), "1397118542")
            elif profil == "reactiver":
                texte = commande(vm, "service status 9")
                confirme(texte, "Current state: REGISTERED")
                confirme(texte, "Autostart: disabled")
                assert generation(texte, "Effective") == generation(texte, "Stored") == conservee
                depend = etat(vm, 10, "WAITING")
                confirme(depend, "9 indisponible")
                confirme(depend, "Autostart: enabled")
                confirme(depend, "Pending restart: no")
                commande(vm, "service start 10")
                confirme(etat(vm, 10, "WAITING"), "9 indisponible")
                confirme(etat(vm, 9, "REGISTERED"), "Autostart: disabled")
                confirme(commande(vm, "cat /documents/exemple.txt"), "NovaOS")
                confirme(commande(vm, "service enable 9"), "enregistré=enabled")
                prochaine = generation(commande(vm, "service status 9"), "Stored")
                assert prochaine > conservee
                vm.vm.kill()
                assert vm.vm.wait(timeout=10) == -signal.SIGKILL
            else:
                texte = etat(vm, 9, "RUNNING")
                assert generation(texte, "Effective") == generation(texte, "Stored") == prochaine
                confirme(texte, "Pending restart: no")
                confirme(commande(vm, "service info 9"), "1397118542")
                etat(vm, 10, "RUNNING")
                premiere = incarnation(vm)
                commande(vm, "service stop 9")
                etat(vm, 9, "STOPPED")
                commande(vm, "service start 9")
                etat(vm, 9, "RUNNING")
                deuxieme = incarnation(vm)
                assert deuxieme != premiere
                commande(vm, "service restart 9")
                fin = time.monotonic() + 30
                while True:
                    etat(vm, 9, "RUNNING")
                    if incarnation(vm) != deuxieme:
                        break
                    assert time.monotonic() < fin, "instance inchangée après restart"
                    time.sleep(0.05)
                confirme(commande(vm, "service info 9"), "1397118542")
            if profil != "reactiver":
                vm.exit()
                vm.until(b"SMP arret motif=normal")
                masque = hex((1 << vcpus) - 1).encode()
                vm.until(b"ack=" + masque + b" absents=0x0 muets=0x0")
        disque = image.read_bytes()
        with gzip.open(serie.with_suffix(".raw.gz"), "wb") as fichier:
            fichier.write(disque)
        with gzip.open(serie.with_suffix(".raw.gz"), "rb") as fichier:
            assert fichier.read() == disque
        verify_image(image, secteur)
        ligne = {"parcours": profil, "sha256_disque": hashlib.sha256(disque).hexdigest(),
                 "sha256_serie": hashlib.sha256(serie.read_bytes()).hexdigest()}
        if admin and profil != "reactiver":
            raw = serie.read_text(errors="replace")
            for phase in ("bloc_repos", "bloc_collecte"):
                detail = next(x for x in raw.splitlines() if f"RAM phase={phase} " in x)
                ligne[phase] = {k: int(v) for k,v in re.findall(r"(\w+)=(\d+)", detail)}
            initial, final = ligne["bloc_repos"], ligne["bloc_collecte"]
            for cle in ("processus", "ipc", "utilisateur", "upile", "kpile", "heap", "buddy", "meta_actifs"):
                assert initial[cle] == final[cle], (cle, ligne)
            assert initial["libres"] - final["libres"] == sum(final[k] - initial[k] for k in ("dma", "tables"))
            ligne["pic_cadres"] = int(re.search(r"pic_cadres=(\d+)", raw)[1])
        bilans.append(ligne)
    (dossier / "resultats.json").write_text(json.dumps(bilans, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64", "x86_64"))
    p.add_argument("vcpus", type=int, choices=(1, 2, 4))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("preuves", type=Path)
    a = p.parse_args()
    cellule(a.arch, a.vcpus, a.secteur, a.preuves.resolve())
