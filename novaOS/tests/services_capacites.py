"""Restrictions des services installés sur P9, observées depuis le véritable nsh."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess

from cli import Terminal
from journalisation import archive, arret, attend, demarrage, lit_evenement, sortie
from services import ROOT, guest
from session_cli import fixture_p9


def etat(vm, service, voulu):
    return attend(lambda: sortie(vm, f"service status {service}"),
                  lambda texte: f"Current state: {voulu}" in texte)


def droits(vm, service, rc=0):
    texte = sortie(vm, f"service capabilities {service}", rc)
    champs = dict(re.findall(r"^(Catalogue|Configuration|Instance|Stored generation|Effective generation|Pending restart): ([^\r\n]+)", texte, re.M))
    assert len(champs) == 6, texte
    for cle in ("Stored generation", "Effective generation"):
        champs[cle] = int(champs[cle])
    return champs


def verifie(champs, maximum, conf, effectif, attente=None):
    assert (champs["Catalogue"], champs["Configuration"], champs["Instance"]) == (
        maximum, conf, effectif), champs
    if attente is not None:
        assert champs["Pending restart"] == attente, champs
    if effectif == "aucune instance active":
        assert champs["Effective generation"] == 0, champs


def changer(vm, verbe, service, droit):
    rep = sortie(vm, f"service capabilities {verbe} {service} {droit}")
    generation = re.search(r"capacité enregistrée génération=(\d+)", rep)
    assert generation, rep
    return int(generation[1])


def memoire(serie):
    raw = serie.read_text(errors="replace")
    phases = {}
    for p in ("bloc_repos", "bloc_collecte"):
        ligne = next(l for l in raw.splitlines() if f"RAM phase={p} " in l)
        phases[p] = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", ligne)}
    debut, fin = phases.values()
    for cle in ("processus", "ipc", "utilisateur", "upile", "kpile", "heap", "buddy", "meta_actifs"):
        assert debut[cle] == fin[cle], (cle, debut, fin)
    assert debut["libres"] - fin["libres"] == sum(fin[k] - debut[k] for k in ("dma", "tables"))
    pic = re.search(r"allocations=(\d+) pic_cadres=(\d+)", raw)
    assert pic, raw[-2000:]
    return dict(**phases, allocations=int(pic[1]), pic_cadres=int(pic[2]))


def journal(vm, service, code, boot):
    return lit_evenement(vm, f"journal show {service} --limit 128", code=code, boot=boot,
                         source="gestionnaire")


def ordinaire(vm):
    etat(vm, 9, "RUNNING")
    assert "service capabilities restrict|restore" in sortie(vm, "service --help")
    vus = {s: droits(vm, s) for s in (9, 10, 16)}
    for s, cap in ((9, "private-tmp"), (10, "cache-backend"), (16, "journal-store")):
        verifie(vus[s], cap, cap, cap, "no")
        for verb in ("restrict", "restore"):
            refus = sortie(vm, f"service capabilities {verb} {s} {cap}", 5)
            assert "accès refusé" in refus and "capacité enregistrée" not in refus, refus
        assert droits(vm, s) == vus[s]
    return vus


def retirer(vm, boot):
    etat(vm, 9, "RUNNING")
    assert "1397118542" in sortie(vm, "service info 9")
    etat(vm, 10, "RUNNING")
    init = droits(vm, 9)
    verifie(init, "private-tmp", "private-tmp", "private-tmp", "no")
    assert "introuvable" in sortie(vm, "service capabilities inconnu", 20)
    sortie(vm, "service capabilities 999", 12)
    sortie(vm, "service capabilities restrict 9 droit-inconnu", 3)
    for verbe in ("restrict", "restore"):
        rejet = sortie(vm, f"service capabilities {verbe} 9 cache-backend", 5)
        assert "capacité enregistrée" not in rejet
    journal(vm, 9, "CAP_DENIED", boot)
    assert droits(vm, 9) == init
    generation = changer(vm, "restrict", 9, "private-tmp")
    avant = droits(vm, 9)
    verifie(avant, "private-tmp", "aucune", "private-tmp", "yes")
    assert generation == avant["Stored generation"] == init["Stored generation"] + 1
    assert init["Effective generation"] == avant["Effective generation"]
    assert "1397118542" in sortie(vm, "service info storage-info")
    journal(vm, 9, "CAP_RESTRICTED", boot)
    sortie(vm, "service restart 9")
    etat(vm, 9, "FAILED")
    assert "9 indisponible" in etat(vm, 10, "WAITING")
    sortie(vm, "service info 9", 12)
    verifie(droits(vm, 9), "private-tmp", "aucune", "aucune instance active")
    assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
    return generation, journal(vm, 9, "CAP_MISSING", boot)


def restaurer(vm, boot, generation):
    etat(vm, 9, "FAILED")
    assert "9 indisponible" in etat(vm, 10, "WAITING")
    apres = droits(vm, 9)
    verifie(apres, "private-tmp", "aucune", "aucune instance active")
    assert apres["Stored generation"] == generation
    assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
    g = changer(vm, "restore", "storage-info", "PRIVATE_TMP")
    assert g == generation + 1
    sortie(vm, "service restart 9")
    etat(vm, 9, "RUNNING")
    etat(vm, 10, "RUNNING")
    effectifs = droits(vm, 9)
    verifie(effectifs, "private-tmp", "private-tmp", "private-tmp", "no")
    assert effectifs["Stored generation"] == effectifs["Effective generation"] == g
    assert "1397118542" in sortie(vm, "service info 9")
    journal(vm, 9, "CAP_RESTORED", boot)
    journal(vm, 9, "CAP_APPLIED", boot)
    return effectifs


def journal_retire(vm, boot):
    init = droits(vm, 16)
    verifie(init, "journal-store", "journal-store", "journal-store", "no")
    gen = changer(vm, "restrict", "journal", "JOURNAL_STORE")
    verifie(droits(vm, 16), "journal-store", "aucune", "journal-store", "yes")
    journal(vm, 16, "CAP_RESTRICTED", boot)
    sortie(vm, "service restart 16")
    etat(vm, 16, "FAILED")
    verifie(droits(vm, 16), "journal-store", "aucune", "aucune instance active")
    sortie(vm, "journal status", (5, 10, 11, 12))
    assert "1397118542" in sortie(vm, "service info 9")
    assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
    assert changer(vm, "restore", 16, "journal-store") == gen + 1
    sortie(vm, "service start 16")
    etat(vm, 16, "RUNNING")
    assert demarrage(vm) == boot
    verifie(droits(vm, 16), "journal-store", "journal-store", "journal-store", "no")
    assert "émission confirmée" in sortie(vm, "journal emit 73")
    return journal(vm, 16, "CAP_MISSING", boot)


def sha(path):
    with path.open("rb") as src:
        return hashlib.file_digest(src, "sha256").hexdigest()


def cellule(arch, cpus, sector, dossier, outil, etendu):
    dossier.mkdir(parents=True, exist_ok=False)
    os.environ["NOVA_VCPUS"] = str(cpus)
    original = dossier / "original.raw"
    image = dossier / "volume.raw"
    fixture_p9(original, sector)
    initial = sha(original)
    subprocess.run([outil, "legacy", original, image, str(sector), "12"], check=True)

    def inspect():
        return subprocess.check_output([outil, "inspect", image, str(sector)])

    slots4 = inspect()
    rows = []
    serie = dossier / "ordinaire.serial"
    with Terminal(arch, sector, image, "capacites-ordinaire", journal=serie) as vm:
        vm.until(b"nsh> ")
        lus = ordinaire(vm)
        assert all(s["Stored generation"] == 12 for s in lus.values()), lus
        arret(vm, cpus)
    assert inspect() == slots4
    rows.append(dict(session="historique", slots=hashlib.sha256(slots4).hexdigest(),
                     droits=lus, **archive(image, sector, serie)))

    serie = dossier / "restrictions.serial"
    with Terminal(arch, sector, image, "capacites-restreindre", profil_journal=True, journal=serie) as vm:
        vm.until(b"nsh> ")
        premier = demarrage(vm)
        gen, evenements = retirer(vm, premier)
        assert gen == 13
        vm.vm.kill()
        assert vm.vm.wait(timeout=10) == -signal.SIGKILL
    rows.append(dict(session="restriction", generation=gen, boot=premier, evenements=evenements,
                     **archive(image, sector, serie)))

    if etendu:
        for op in ("corrupt-latest", "corrupt-authority"):
            copie = dossier / (op + ".raw")
            subprocess.run([outil, op, image, copie, str(sector), str(gen)], check=True)
            trace = dossier / (op + ".serial")
            with Terminal(arch, sector, copie, "capacites-"+op, profil_journal=True, journal=trace) as vm:
                vm.until(b"nsh> ")
                assert "NovaOS" in sortie(vm, "cat /documents/exemple.txt")
                vus = {s: droits(vm, s, 21) for s in (9, 10, 16)}
                for s, cap in ((9, "private-tmp"), (10, "cache-backend"), (16, "journal-store")):
                    verifie(vus[s], cap, "aucune", "aucune instance active")
                assert "Configuration de sécurité non vérifiable" in sortie(vm, "service start 9", 21)
                sortie(vm, "service capabilities restore 9 PRIVATE_TMP", 21)
                arret(vm, cpus)
            rows.append(dict(session=op, droits=vus, memoire=memoire(trace), **archive(copie, sector, trace)))

    serie = dossier / "restauration.serial"
    with Terminal(arch, sector, image, "capacites-restaurer", profil_journal=True, journal=serie) as vm:
        vm.until(b"nsh> ")
        boot = demarrage(vm)
        assert boot != premier
        droits_retablis = restaurer(vm, boot, gen)
        evenements = journal_retire(vm, boot)
        arret(vm, cpus)
    rows.append(dict(session="restauration", boot=boot, droits=droits_retablis, evenements=evenements,
                     memoire=memoire(serie), **archive(image, sector, serie)))
    assert initial == sha(original)
    resultat = dict(architecture=arch, vcpu=cpus, secteur=sector, noyau=sha(Path(os.environ["NOVA_KERNEL_ELF"])),
                    outil=sha(outil), original=initial, sessions=rows)
    (dossier / "resultats.json").write_text(json.dumps(resultat, ensure_ascii=False, indent=2) + "\n")


def course(arch, cpus, sector, dossier, outil):
    dossier.mkdir(parents=True, exist_ok=False)
    os.environ["NOVA_VCPUS"] = str(cpus)
    original, image = dossier / "original.raw", dossier / "course.raw"
    fixture_p9(original, sector)
    initial = sha(original)
    subprocess.run([outil, "legacy", original, image, str(sector), "12"], check=True)
    iso = dossier / "course.iso"
    env = dict(os.environ, NOVA_ARCH=arch)
    subprocess.run(["sh", ROOT / "scripts/iso.sh", "services-config-race", iso],
                   env=env, stdout=subprocess.DEVNULL, check=True)
    log = dossier / "course.serial"
    guest(arch, sector, cpus, image, iso, "services-capacites-race", log, False)
    apres_course = archive(image, sector, log)
    suite = dossier / "relecture.serial"
    with Terminal(arch, sector, image, "capacites-relecture-course", journal=suite) as vm:
        vm.until(b"nsh> ")
        etat(vm, 9, "RUNNING")
        etat(vm, 10, "RUNNING")
        relus = droits(vm, 9)
        assert relus["Effective generation"] == relus["Stored generation"] == 14, relus
        verifie(relus, "private-tmp", "private-tmp", "private-tmp", "no")
        assert "1397118542" in sortie(vm, "service info 9")
        arret(vm, cpus)
    assert sha(original) == initial
    resume = dict(architecture=arch, vcpu=cpus, secteur=sector, generation=14, droits=relus,
                  initial=initial, course=apres_course, relecture=archive(image, sector, suite),
                  noyau=sha(Path(os.environ["NOVA_KERNEL_ELF"])), outil=sha(outil), iso=sha(iso),
                  memoire=memoire(log))
    (dossier / "resultats.json").write_text(json.dumps(resume, ensure_ascii=False, indent=2)+"\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64", "x86_64"))
    p.add_argument("vcpu", type=int, choices=(1, 2, 4))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("preuves", type=Path)
    p.add_argument("outil", type=Path)
    modes = p.add_mutually_exclusive_group()
    modes.add_argument("--corruptions", action="store_true")
    modes.add_argument("--course", action="store_true")
    a = p.parse_args()
    params = (a.arch, a.vcpu, a.secteur, a.preuves.resolve(), a.outil.resolve(strict=True))
    if a.course:
        course(*params)
    else:
        cellule(*params, a.corruptions)
