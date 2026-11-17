"""Douze comparaisons Darwin sur un fichier binaire local, sous le même argv et le même environnement."""
import argparse
import datetime
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time

from mesures import paths
from options import GNU, NOVA, sha

BINAIRES = {"Nova": NOVA, "GNU": GNU}
ENV = {"LC_ALL": "C"}
PASSAGES = 15
LANCEMENTS = 16
SEUIL = Fraction(11, 10)


def empreinte(data):
    return hashlib.sha256(data).hexdigest()


def generations():
    resultat = {}
    for nom, chemin in BINAIRES.items():
        stat = chemin.stat()
        resultat[nom] = dict(chemin=str(chemin.resolve()), sha256=sha(chemin),
                             device=stat.st_dev, inode=stat.st_ino, taille=stat.st_size,
                             mtime_ns=stat.st_mtime_ns, ctime_ns=stat.st_ctime_ns)
    return resultat


def conditions():
    ps = subprocess.check_output(["ps", "-Ao", "comm="], text=True)
    termes = ["qemu-system", "cargo", "rustc", "python", "gbasename", "basename-darwin"]
    return dict(date=datetime.datetime.now().astimezone().isoformat(), charge=os.getloadavg(),
                processus=[Path(p.strip()).name for p in ps.splitlines() if any(t in p.lower() for t in termes)])


def charge(n, option):
    argv = ["basename", option, *paths(n)]
    suffixe = "" if "s" in option else ".txt"
    fin = b"\0" if "z" in option else b"\n"
    attendu = b"".join(f"document-{i:05}{suffixe}".encode() + fin for i in range(n))
    return argv, attendu


def capture(argv, attendu):
    resultat = {}
    for nom, binaire in BINAIRES.items():
        p = subprocess.run(argv, executable=binaire, capture_output=True, env=ENV)
        if (p.returncode, p.stdout, p.stderr) != (0, attendu, b""):
            raise ValueError((nom, argv[:3], p.returncode, p.stdout[:60], p.stderr))
        resultat[nom] = dict(statut=p.returncode, stdout_sha256=empreinte(p.stdout),
                             octets=len(p.stdout), stderr_hex=p.stderr.hex())
    return resultat


def passage(binaire, argv, attendu, sortie, erreur, repetitions):
    for canal in (sortie, erreur):
        canal.seek(0)
        canal.truncate(0)
    actions = [(os.POSIX_SPAWN_DUP2, sortie.fileno(), 1), (os.POSIX_SPAWN_DUP2, erreur.fileno(), 2)]
    pic = 0
    debut = time.perf_counter_ns()
    for _ in range(repetitions):
        pid = os.posix_spawn(binaire, argv, ENV, file_actions=actions)
        _, statut, usage = os.wait4(pid, 0)
        if statut:
            raise RuntimeError((binaire, statut))
        pic = max(pic, usage.ru_maxrss)
    duree = time.perf_counter_ns() - debut
    sortie.seek(0)
    erreur.seek(0)
    if sortie.read() != attendu * repetitions or erreur.read():
        raise ValueError("Flux inattendu dans le lot chronométré")
    return dict(ns=duree, rss_max_octets=pic)


def mediane(valeurs):
    a = sorted(valeurs)
    return Fraction(a[(len(a)-1)//2] + a[len(a)//2], 2)


def qualifier(n, option, sortie, erreur, inverse):
    argv, attendu = charge(n, option)
    avant = capture(argv, attendu)
    for binaire in BINAIRES.values():
        passage(binaire, argv, attendu, sortie, erreur, 4)
    bruts = []
    for i in range(PASSAGES):
        ordre = list(BINAIRES)
        if (i + inverse) % 2:
            ordre.reverse()
        ligne = dict(passage=i+1, ordre=ordre)
        for nom in ordre:
            ligne[nom] = passage(BINAIRES[nom], argv, attendu, sortie, erreur, LANCEMENTS)
        bruts.append(ligne)
    argv_apres, attendu_apres = charge(n, option)
    apres = capture(argv_apres, attendu_apres)
    if argv != argv_apres or avant != apres or attendu != attendu_apres:
        raise ValueError("Génération différente en fin de cellule")
    medianes, dispersion = {}, {}
    for nom in BINAIRES:
        valeurs = [r[nom]["ns"] for r in bruts]
        medianes[nom] = mediane(valeurs) / LANCEMENTS
        dispersion[nom] = dict(min_ms=min(valeurs)/LANCEMENTS/1e6, max_ms=max(valeurs)/LANCEMENTS/1e6,
                               mad_ms=float(mediane([abs(x-mediane(valeurs)) for x in valeurs])/LANCEMENTS)/1e6)
    rapport = medianes["Nova"] / medianes["GNU"]
    return dict(n=n, option=option, argv_sha256=empreinte(b"\0".join(map(os.fsencode,argv))+b"\0"),
                avant=avant, apres=apres, echantillons=bruts, dispersion=dispersion,
                medianes_ns={k:str(v) for k,v in medianes.items()}, rapport_exact=str(rapport),
                conforme=rapport<=SEUIL)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("dossier", type=Path, help="nouveau répertoire de preuves")
    out = p.parse_args().dossier
    out.mkdir(parents=True, exist_ok=False)
    resultats = dict(machine=platform.machine(), systeme=platform.platform(), locale="C",
                     version_gnu=subprocess.check_output([GNU,"--version"],env=ENV,text=True).splitlines()[0],
                     commit=subprocess.check_output(["git","-C",NOVA.parents[2],"rev-parse","HEAD"],text=True).strip(),
                     seuil=str(SEUIL), passages=PASSAGES, lancements=LANCEMENTS,
                     echauffement=4, debut=conditions(), generations_avant=generations(), cellules=[])
    with tempfile.TemporaryFile(dir=os.environ["TMPDIR"]) as sortie, tempfile.TemporaryFile(dir=os.environ["TMPDIR"]) as err:
        for n in [1,30,512]:
            for opt in ["-a","-az","-s.txt","-zs.txt"]:
                cellule = qualifier(n,opt,sortie,err,len(resultats["cellules"]) % 2)
                resultats["cellules"].append(cellule)
                print(n,opt,cellule["rapport_exact"],cellule["conforme"],flush=True)
    resultats.update(fin=conditions(), generations_apres=generations())
    (out/"mesures.json").write_text(json.dumps(resultats,ensure_ascii=False,indent=1)+"\n")
    if resultats["generations_avant"] != resultats["generations_apres"]:
        raise RuntimeError("Génération binaire modifiée pendant la campagne")
    return 0 if all(r["conforme"] for r in resultats["cellules"]) else 1


if __name__ == "__main__":
    raise SystemExit(main())
