"""Journal natif sur P9 : accès ordinaire, cycle réel 9/10 et relecture de trois BootID."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import time

from cli import Terminal, verify_image
from services_configuration import etat
from session_cli import fixture_p9


def sortie(vm, ligne, statut=0):
    # Une trace noyau peut interrompre l'écho, puis l'éditeur redessine « nsh> ».
    # Seuls les deux marqueurs exécutés par l'interpréteur délimitent la sortie.
    vm.vm.stdin.write(f"nsh -c 'echo journaldebut; {ligne}; echo journalrc=$?'\n".encode())
    vm.until(b"\r\njournaldebut", timeout=180)
    contenu = vm.until(b"\r\njournalrc=", timeout=180)
    fin = vm.until(b"\r\n")
    vm.until(b"nsh> ")
    texte = (contenu + fin).decode("utf-8", errors="replace")
    permis = (statut,) if isinstance(statut, int) else statut
    rc = fin.strip()
    assert rc.isdigit() and int(rc) in permis, (ligne, statut, texte[-900:])
    return texte


def evenements(texte):
    result = [dict(re.findall(r"(\w+)=([^\s]+)", line))
              for line in texte.splitlines() if line.startswith("seq=")]
    seq = [int(row["seq"]) for row in result]
    assert seq == sorted(set(seq)), seq
    return result


def attend(lire, accepte):
    # Répéter uniquement une lecture d'observation, aucune commande de mutation.
    fin = time.monotonic() + 90
    while True:
        resultat = lire()
        if accepte(resultat):
            return resultat
        if time.monotonic() >= fin:
            raise AssertionError(("observation incomplète", resultat))
        time.sleep(0.1)


def statut(vm):
    texte = sortie(vm, "journal status", (0, 12))
    return dict(re.findall(r"(\w+)=([^\s]+)", texte))


def demarrage(vm):
    courant = attend(lambda: statut(vm), lambda s: re.fullmatch(r"[0-9a-f]{32}", s.get("boot", "")))
    assert int(courant["boot"], 16) != 0
    return courant["boot"]


def rows(vm, ligne):
    return evenements(sortie(vm, ligne))


def contient(tab, **critere):
    return any(all(e[k] == v for k, v in critere.items()) for e in tab)


def lit_evenement(vm, ligne, **critere):
    return attend(lambda: rows(vm, ligne), lambda tab: contient(tab, **critere))


def archive(image, secteur, serie):
    generation = verify_image(image, secteur)
    copie = serie.with_suffix(".raw.gz")
    with image.open("rb") as src, gzip.open(copie, "xb") as dst:
        shutil.copyfileobj(src, dst)
    with image.open("rb") as src, gzip.open(copie, "rb") as dst:
        empreinte = hashlib.file_digest(src, "sha256").hexdigest()
        assert hashlib.file_digest(dst, "sha256").hexdigest() == empreinte
    return {"generation_p9": generation, "sha256_disque": empreinte,
            "sha256_serie": hashlib.sha256(serie.read_bytes()).hexdigest()}


def arret(vm, vcpus):
    vm.exit()
    vm.until(b"SMP arret motif=normal")
    vm.until(f"ack={hex((1 << vcpus) - 1)} absents=0x0 muets=0x0".encode())


def ordinaire(arch, vcpus, secteur, dossier, temoin):
    # Aucun historique d'un ancien essai ne peut changer les attentes de ce volume.
    image = dossier / "ordinaire.raw"
    shutil.copyfile(temoin, image)
    serie = dossier / "ordinaire.serial"
    with Terminal(arch, secteur, image, "journal-ordinaire", journal=serie) as vm:
        vm.until(b"nsh> ")
        etat(vm, 9, "RUNNING")
        assert "history journal ls" in sortie(vm, "help")
        assert "journal tail" in sortie(vm, "journal --help")
        attend(lambda: sortie(vm, "journal status", (5, 12)), lambda t: "accès refusé (5)" in t)
        # READDIR lie ses pages à la génération P9. Une publication du gestionnaire
        # entre deux pages invalide cette énumération avant son terme.
        listing = attend(lambda: sortie(vm, "ls -a /", (0, 10)), lambda t: "journalrc=0" in t)
        assert "documents" in listing and "NovaOS-journal" not in listing
        assert "introuvable" in sortie(vm, "cat /NovaOS-journal", 20)
        assert rows(vm, "journal show self") == []
        assert rows(vm, "journal tail self --limit 1") == []
        for cmd in ("list", "status", "show 9", "tail cache", "boot",
                    "emit 17", "rotate", "retain 2", "purge"):
            texte = sortie(vm, f"journal {cmd}", 5)
            assert "accès refusé" in texte and not evenements(texte) and "révision=" not in texte
        assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
        arret(vm, vcpus)
    return {"session": "ordinaire", **archive(image, secteur, serie)}


def profil_admin(arch, vcpus, secteur, dossier, image):
    compte_rendu, boots, historiques = [], [], []
    for i, parcours in enumerate(("desactivation", "dependance", "reactivation")):
        serie = dossier / f"{i + 1}-{parcours}.serial"
        with Terminal(arch, secteur, image, f"journal-{parcours}", journal=serie,
                      profil_journal=True) as vm:
            vm.until(b"nsh> ")
            boot = demarrage(vm)
            assert boot not in boots, (boots, boot)
            boots.append(boot)
            assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
            premiers = lit_evenement(vm, "journal boot --limit 128", boot=boot, code="BOOT")
            marque, = (e for e in premiers if e["code"] == "BOOT")
            bloc, gestionnaire = map(int, marque["données"].split(","))
            assert marque["source"] == "gestionnaire" and int(marque["ms"]) <= bloc <= gestionnaire
            assert "Journal gestionnaire: perdus=" in sortie(vm, "service status 16")
            assert rows(vm, "journal show self") == [], "notice d'une autre instance divulguée"
            if i == 0:
                etat(vm, 9, "RUNNING")
                etat(vm, 10, "RUNNING")
                lit_evenement(vm, "journal show storage-info", code="STATE", état="READY", source="gestionnaire")
                assert "1397118542" in sortie(vm, "service info 9")
                assert "enregistré=disabled" in sortie(vm, "service disable 9")
                etat(vm, 9, "RUNNING")
                lit_evenement(vm, "journal show 9", code="DISABLED", source="gestionnaire")
                # Les deux identités (service et émetteur) ont un sens différent.
                lit_evenement(vm, "journal show 9", code="NOTICE", source="producteur")
            elif i == 1:
                assert "Autostart: disabled" in etat(vm, 9, "REGISTERED")
                assert "9 indisponible" in etat(vm, 10, "WAITING")
                avertis = lit_evenement(vm, "journal show cache --level WARN", code="DEPENDENCY_WAIT", boot=boot)
                assert all(e["niveau"] in ("WARN", "ERROR", "CRITICAL") for e in avertis)
                assert "enregistré=enabled" in sortie(vm, "service enable 9")
                lit_evenement(vm, "journal show 9", code="ENABLED", boot=boot)
            else:
                etat(vm, 9, "RUNNING")
                etat(vm, 10, "RUNNING")
                assert "1397118542" in sortie(vm, "service info 9")
                lit_evenement(vm, "journal show cache --limit 128", boot=boot, code="STATE", état="READY")
            debut = time.monotonic_ns()
            texte = sortie(vm, f"journal emit {151 + i}")
            latence = time.monotonic_ns() - debut
            eid = re.search(r"émission confirmée eid=(\d+)", texte)[1]
            propres = rows(vm, "journal show self")
            assert len(propres) == 1 and propres[0]["seq"] == propres[0]["eid"] == eid
            assert propres[0]["boot"] == boot and propres[0]["données"] == f"{151 + i},0"
            assert rows(vm, "journal tail self --limit 1") == propres
            sortie(vm, "journal emit 256", 3)
            sortie(vm, "journal boot --boot zzzz", 3)
            sortie(vm, "journal show 9 --limit 129", 3)
            assert rows(vm, "journal show self --level CRITICAL") == []
            # Conserver une coupe exacte avant l'arrêt. La lecture ultérieure peut inclure SHUTDOWN.
            courant = rows(vm, "journal boot --limit 128")
            assert courant and all(e["boot"] == boot for e in courant)
            historiques.append(courant)
            if i == 2:
                for ancien, capture in zip(boots, historiques):
                    relu = rows(vm, f"journal boot --boot {ancien} --limit 128")
                    assert relu[:len(capture)] == capture, ("historique P9 modifié", ancien, len(capture), len(relu))
                    if ancien != boot:
                        assert contient(relu, code="SHUTDOWN", source="gestionnaire"), ancien
                neuf = rows(vm, "journal show 9 --limit 128")
                dix = rows(vm, "journal show cache --limit 128")
                assert all(e["service"] == "9" for e in neuf) and all(e["service"] == "10" for e in dix)
                assert contient(neuf, boot=boots[0], code="STATE", état="READY")
                assert contient(neuf, boot=boots[0], code="DISABLED")
                assert contient(dix, boot=boots[1], code="DEPENDENCY_WAIT")
                assert contient(neuf, boot=boots[1], code="ENABLED")
                assert contient(neuf, boot=boots[2], code="STATE", état="READY")
                assert contient(dix, boot=boots[2], code="STATE", état="READY")
                assert rows(vm, "journal tail 9 --limit 2") == neuf[-2:]
                assert "service=9 événements=" in sortie(vm, "journal list")
                vm.vm.stdin.write(b"journal tail self --follow\n")
                vm.until(b"suivi du journal (q pour quitter)")
                time.sleep(0.55)
                vm.vm.stdin.write(b"q")
                fin = vm.until(b"nsh> ")
                assert b"seq=" not in fin, fin
            arret(vm, vcpus)
        compte_rendu.append({"session": parcours, "boot": boot, "eid_confirme": eid,
                             "latence_emission_hote_ns": latence, "evenements": courant,
                             **archive(image, secteur, serie)})
    return compte_rendu


def rotation(arch, vcpus, secteur, dossier, source):
    # Les administrateurs exercent la purge sur un clone indépendant seulement.
    image = dossier / "retention.raw"
    shutil.copyfile(source, image)
    serie = dossier / "retention.serial"
    with Terminal(arch, secteur, image, "journal-retention", journal=serie, profil_journal=True) as vm:
        vm.until(b"nsh> ")
        demarrage(vm)
        etat(vm, 10, "RUNNING")
        # Les notifications de démarrage sont asynchrones. Attendre les événements
        # effectivement confirmés avant de mesurer une mutation à révision attendue.
        lit_evenement(vm, "journal boot --service 10 --limit 128", code="STATE", état="READY")
        precedents = statut(vm)
        fin = time.monotonic() + 30
        while True:
            time.sleep(0.5)
            suivants = statut(vm)
            if precedents.get("révision") == suivants["révision"]:
                break
            assert time.monotonic() < fin, (precedents, suivants)
            precedents = suivants
        # Un segment nouveau après la 17e notice implique une rotation automatique.
        assert "retain confirmé" in sortie(vm, "journal retain 2")
        assert "purge confirmé" in sortie(vm, "journal purge")
        eids, latences = [], []
        for n in range(33):
            t = time.monotonic_ns()
            texte = sortie(vm, f"journal emit {n}")
            latences.append(time.monotonic_ns() - t)
            eids.append(re.search(r"eid=(\d+)", texte)[1])
        st = statut(vm)
        assert "pertes" not in st  # Les diagnostics de perte appartiennent au gestionnaire.
        assert int(st["segments"]) == 2
        assert int(st["octets"]) <= int(st["quota"]) == 2 * 16 * 96
        avant = rows(vm, "journal show self --limit 128")
        assert 0 < len(avant) <= 32
        assert [e["eid"] for e in avant] == eids[-len(avant):]
        assert avant[-1]["données"] == "32,0" and avant[0]["eid"] != eids[0]
        # Lecture système inverse épuisée après éviction de la première séquence.
        assert int(st["premier"]) > 1
        assert "enregistré=disabled" in sortie(vm, "service disable 9")
        visibles = lit_evenement(vm, "journal show 9 --limit 128", code="DISABLED")
        assert 0 < len(visibles) < 128
        assert rows(vm, "journal tail 9 --limit 128") == visibles

        t = time.monotonic_ns()
        assert "rotate confirmé" in sortie(vm, "journal rotate")
        rotation_ns = time.monotonic_ns() - t
        assert "purge confirmé" in sortie(vm, "journal purge")
        assert rows(vm, "journal show self --limit 128") == []
        assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
        assert "1397118542" in sortie(vm, "service info 9")
        arret(vm, vcpus)
    return {"session": "rotation", "latences_emission_hote_ns": latences,
            "rotation_hote_ns": rotation_ns, "saturation": st, **archive(image, secteur, serie)}


def cellule(arch, vcpus, secteur, dossier, stress=False):
    dossier.mkdir(parents=True, exist_ok=False)
    os.environ["NOVA_VCPUS"] = str(vcpus)
    image = dossier / "volume.raw"
    fixture_p9(image, secteur)
    bilan = [ordinaire(arch, vcpus, secteur, dossier, image)]
    bilan.extend(profil_admin(arch, vcpus, secteur, dossier, image))
    (dossier / "cycle.json").write_text(json.dumps(bilan, ensure_ascii=False, indent=2) + "\n")
    if stress:
        bilan.append(rotation(arch, vcpus, secteur, dossier, image))
    (dossier / "resultats.json").write_text(json.dumps(bilan, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64", "x86_64"))
    p.add_argument("vcpus", type=int, choices=(1, 2, 4))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("preuves", type=Path)
    scenario = p.add_mutually_exclusive_group()
    scenario.add_argument("--rotation", action="store_true", help="Ajouter la saturation et la purge sur une copie.")
    scenario.add_argument("--rotation-source", type=Path,
                          help="Rejouer uniquement la rotation sur une copie du volume qualifié.")
    a = p.parse_args()
    dossier = a.preuves.resolve()
    if a.rotation_source is None:
        cellule(a.arch, a.vcpus, a.secteur, dossier, a.rotation)
    else:
        dossier.mkdir(parents=True, exist_ok=False)
        os.environ["NOVA_VCPUS"] = str(a.vcpus)
        source = a.rotation_source.resolve(strict=True)
        with source.open("rb") as disque:
            avant = hashlib.file_digest(disque, "sha256").hexdigest()
        bilan = rotation(a.arch, a.vcpus, a.secteur, dossier, source)
        with source.open("rb") as disque:
            assert hashlib.file_digest(disque, "sha256").hexdigest() == avant
        bilan["sha256_volume_source"] = avant
        (dossier / "resultats.json").write_text(json.dumps(bilan, ensure_ascii=False, indent=2) + "\n")
