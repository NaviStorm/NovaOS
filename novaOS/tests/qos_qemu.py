"""Qualifier la QoS du thread TCG de QEMU direct sur Darwin, avec les deux géométries P9."""
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import itertools
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile

from vcpu_phases_tcg import symboles

POLITIQUES = ("ordinaire", "clamp-utility", "clamp-background", "thread-background", "thread-user-initiated")
CAS = ("ajout-4m-multiple", "inserer-milieu")
ARCHS = ("aarch64", "x86_64")
CHAMPS = ("indice", "fil", "coeur", "qos_demandee", "relative", "priorite_base", "priorite_courante",
          "cpu_ns", "ecoule_ns")


def empreinte(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def instant():
    return datetime.now(timezone.utc).isoformat()


def topologie():
    arbre = plistlib.loads(subprocess.check_output(["/usr/sbin/ioreg", "-arl", "-p", "IODeviceTree",
                                                    "-n", "cpus", "-d", "2"]))
    classes = {}
    for branche in arbre:
        for coeur in branche["IORegistryEntryChildren"]:
            numero = coeur["logical-cpu-id"]
            nature = coeur["cluster-type"].rstrip(b"\0").decode("ascii")
            if numero in classes or nature not in ("P", "E"):
                raise ValueError("Topologie Apple non univoque")
            classes[numero] = nature
    if not classes or set(classes) != set(range(os.cpu_count())):
        raise ValueError("Topologie Apple incomplète")
    return classes


def synthetiser(trace, classes, politique):
    points = []
    for ligne in trace.read_text().splitlines():
        type_, *valeurs = ligne.split()
        point = dict(type=type_, **dict(zip(CHAMPS, map(int, valeurs), strict=True)))
        point["type_coeur"] = classes[point["coeur"]]
        points.append(point)
    assert [p["type"] for p in points[:3]] == ["I", "Q", "S"] and points[-1]["type"] == "T"
    assert len({p["fil"] for p in points}) == 1
    attendu = {"thread-user-initiated": 0x19, "thread-background": 0x09}.get(politique)
    if attendu is not None:
        assert all(p["qos_demandee"] == attendu for p in points[1:])
    bornes = [p for p in points if p["type"] == "B"]
    assert [p["indice"] for p in bornes] == [0, 1, 2, 3]
    def intervalle(debut, fin):
        return dict(cpu_ms=(fin["cpu_ns"] - debut["cpu_ns"])/1e6,
                    ecoule_ms=(fin["ecoule_ns"] - debut["ecoule_ns"])/1e6)
    return dict(publication=intervalle(points[2], points[-1]), F1=intervalle(*bornes[:2]),
                F2=intervalle(*bornes[2:]), echantillons=dict(Counter(p["type_coeur"] for p in points[2:])),
                points=points)


def essai(args, arch, secteur, cas, politique, dossier, classes):
    archive = args.archive.resolve() / "novaOS"
    sys.path.insert(0, str(archive / "tests"))
    import cli
    from es_groupees_elargies import essai as sauvegarder
    cible = "aarch64-unknown-none-softfloat" if arch == "aarch64" else "x86_64-unknown-none"
    elf = archive / "target" / cible / "release/novaos"
    entree = re.search(r"Entry point address:\s+0x([0-9a-f]+)",
                       subprocess.check_output(["llvm-readelf", "-h", elf], text=True))[1]
    marker, taille, actif, phases, _, barrieres = symboles(elf)
    classe = politique.removeprefix("thread-") if politique.startswith("thread-") else "observe"
    trace = dossier / "points.txt"
    options = [str(args.plugin.resolve()), *("addr="+n for n in (entree, marker, taille, actif, phases, barrieres)),
               "class="+classe, "out="+str(trace)]
    if any("," in chaine for chaine in options):
        raise ValueError("Virgule réservée par QEMU")
    prefixe = ["/usr/sbin/taskpolicy", "-c", politique[6:]] if politique.startswith("clamp-") else []
    machine = cli.machine
    def avec_sonde(*a, **kw):
        cmd, env = machine(*a, **kw)
        return [*prefixe, *cmd, "-plugin", ",".join(options)], env
    cli.machine = avec_sonde
    try:
        with tempfile.TemporaryDirectory() as tmp:
            mesure = sauvegarder(arch, secteur, cas, args.generateur.resolve(), Path(tmp))
    finally:
        cli.machine = machine
        sys.path.remove(str(archive / "tests"))
    serie = archive / "build" / f"cli-{arch}-{secteur}-elargi-{cas}.serial"
    shutil.copyfile(serie, dossier / "invité.serial")
    compteur = mesure["compteur"]
    assert compteur["total_avec_controles_locaux"] <= 64 and compteur["bloc"]["flush"]["nombre"] == 2
    assert mesure["apres"][-1] > mesure["avant"][-1]
    return dict(sha256_elf=empreinte(elf), sha256_serie=empreinte(serie), mesure=mesure,
                qos=synthetiser(trace, classes, politique))


def matrice(repetitions):
    axes = list(itertools.product(ARCHS, CAS))
    for rep in range(repetitions):
        for arch, cas in axes[rep % len(axes):] + axes[:rep % len(axes)]:
            i = axes.index((arch, cas))
            secteur = (512, 4096)[(rep + i) % 2]
            j = (rep + i) % len(POLITIQUES)
            rotation = POLITIQUES[j:] + POLITIQUES[:j]
            for politique in (rotation if rep % 2 == 0 else reversed(rotation)):
                yield rep+1, arch, secteur, cas, politique


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for nom in ("archive", "plugin", "generateur", "sortie"):
        p.add_argument(nom, type=Path)
    p.add_argument("--repetitions", type=int, default=6, choices=range(1, 51))
    p.add_argument("--pilote", choices=POLITIQUES)
    p.add_argument("--arch", choices=ARCHS, default="aarch64", help="architecture pour --pilote")
    args = p.parse_args()
    out = args.sortie.resolve()
    out.mkdir(parents=True, exist_ok=True)
    classes = topologie()
    environnement = dict(darwin=platform.release(), version_macos=platform.mac_ver()[0],
                         modele=subprocess.check_output(["sysctl", "-n", "hw.model"], text=True).strip(),
                         processeur=subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip(),
                         qemu={a: subprocess.check_output(["qemu-system-"+a, "--version"], text=True).splitlines()[0]
                               for a in ARCHS}, topologie=classes, sha256_plugin=empreinte(args.plugin),
                         sha256_generateur=empreinte(args.generateur))
    (out / "environnement.json").write_text(json.dumps(environnement, indent=2, ensure_ascii=False)+"\n")
    essais = [(0, args.arch, 4096, CAS[0], args.pilote)] if args.pilote else matrice(args.repetitions)
    for ordre, (rep, arch, s, cas, pol) in enumerate(essais, 1):
        dossier = out / f"{ordre:03d}-{arch}-{s}-{cas}-{pol}"
        dossier.mkdir()
        r = dict(ordre=ordre, repetition=rep, architecture=arch, secteur=s, cas=cas, politique=pol,
                 topologie=classes, debut=instant(), charge_avant=os.getloadavg())
        try:
            r.update(essai(args, arch, s, cas, pol, dossier, classes))
        except (AssertionError, OSError, ValueError, RuntimeError, subprocess.SubprocessError):
            import traceback
            r["erreur"] = traceback.format_exc()
        r.update(fin=instant(), charge_apres=os.getloadavg())
        (dossier / "essai.json").write_text(json.dumps(r, ensure_ascii=False, separators=(",", ":"))+"\n")
        print(dossier.name, r.get("mesure", {}).get("sauvegarde_ms", r.get("erreur")), flush=True)
        if args.pilote and "erreur" in r:
            sys.exit(1)


if __name__ == "__main__":
    main()
