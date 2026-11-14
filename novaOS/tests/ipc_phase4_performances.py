"""Comparer des noyaux ipc-smp-lab, un seul invité à la fois, avec le même banc."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import tarfile

from ipc_multicoeur import CIBLES, mesurer

VERSIONS = {
    "historique": "f356970d27cf8e8fad24d487a3debdc4d2a0be18",
    "phase3": "4e741fbb2e40df84f69f1a2e8ee40b5182968650",
    "phase4": "c34a54f8ea622c658bcc6ce30693a0a02c6d2a35",
}
BANC = ("novaOS/kernel/src/sched/process/ipc/probes/multicore.rs", "novaOS/user/ipc.rs")
CAS = ("libre", "local", "distant", "paires", "paires-cpu0", "paires-libres", "paires-sonde", "sonde")


def construire(dossier):
    racine = Path(__file__).resolve().parents[2]
    compilateur = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"],
                                          text=True).strip()
    for nom, rev in VERSIONS.items():
        source = dossier.resolve() / "sources" / nom
        source.mkdir(parents=True, exist_ok=False)
        commande = ["git", "archive", rev, "novaOS", "novaFS", "components",
                    ":(exclude)novaOS/docs/**", ":(exclude)novaFS/docs/**"]
        with subprocess.Popen(commande, cwd=racine, stdout=subprocess.PIPE) as archive:
            with tarfile.open(fileobj=archive.stdout, mode="r|") as contenu:
                contenu.extractall(source, filter="data")
            if archive.wait():
                raise RuntimeError(f"Export Git incomplet pour {rev}")
        for chemin in BANC:
            contenu = subprocess.check_output(["git", "show", f"{VERSIONS['phase4']}:{chemin}"], cwd=racine)
            (source / chemin).write_bytes(contenu)
        for cible in CIBLES.values():
            subprocess.run(["rustup", "run", "stable", "cargo", "build", "--release", "--locked",
                            "--offline", "--features", "ipc-smp-lab", "--target", cible,
                            "--target-dir", dossier.resolve() / nom], cwd=source / "novaOS",
                           env=os.environ | {"RUSTC": compilateur}, check=True)


def quantiles(valeurs):
    valeurs = sorted(valeurs)

    def rang(fraction):
        return valeurs[math.ceil(fraction * len(valeurs)) - 1]

    return dict(p50=statistics.median(valeurs), p95=rang(.95), p99=rang(.99),
                min=valeurs[0], max=valeurs[-1], iqr=rang(.75) - rang(.25))


def relever(donnees):
    lots = []
    for precedent, courant in zip(donnees["releves"], donnees["releves"][1:]):
        lot = dict(invite_ns=courant["ns"], hote_ns=courant["hote_ns"], couples_ns=courant["couples"])
        if courant["cpu"]:
            cpu = list(zip(precedent["cpu"], courant["cpu"], strict=True))
            noms = ("acquisitions", "conflits", "attente", "detention", "switch", "migrations",
                    "ipi", "tlb", "ticks_usr", "ticks_sys", "ticks_idle", "attente_ipc")
            compteurs = {nom: sum(fin[nom] - debut[nom] for debut, fin in cpu) for nom in noms}
            lot.update(compteurs=compteurs, reveils_observes=courant["detail"]["reprises"],
                       aller_retour_ns=courant["hist"]["aller_retour"])
        lots.append(lot)
    return dict(rep=donnees["repetition"], elf=donnees["elf"], hz=donnees["config"]["hz"], lots=lots,
                charge_fin=donnees["charge_fin"], cpu_hote_user_s=donnees["cpu_hote_user_s"],
                cpu_hote_sys_s=donnees["cpu_hote_sys_s"])


def statistiques(boots):
    lots = [lot for boot in boots for lot in boot["lots"]]
    n = len(lots[0]["couples_ns"])
    # Une fenêtre contient N couples, chacun exécutant exactement 1 000 tours.
    durees = [p["invite_ns"] for p in lots]
    mesures = dict(us_tour=quantiles([ns / 1e6 for ns in durees]),
                   tours_s=quantiles([n * 1e12 / ns for ns in durees]),
                   hote_tours_s=quantiles([n * 1e12 / p["hote_ns"] for p in lots]),
                   hote_ms_lot=quantiles([p["hote_ns"] / 1e6 for p in lots]),
                   couples_tours_s=[quantiles([1e12 / p["couples_ns"][i] for p in lots]) for i in range(n)],
                   cpu_hote_s_vm=quantiles([b["cpu_hote_user_s"] + b["cpu_hote_sys_s"] for b in boots]),
                   debits_medians_par_demarrage=[
                       statistics.median(n * 1e12 / p["invite_ns"] for p in b["lots"]) for b in boots])
    if "compteurs" in lots[0]:
        mesures["garde_ms"] = {
            k: quantiles([lot["compteurs"][k] * 1000 / b["hz"] for b in boots for lot in b["lots"]])
            for k in ("attente", "detention")}
        mesures["compteurs_medians"] = {
            k: statistics.median(p["compteurs"][k] for p in lots) for k in lots[0]["compteurs"]}
    return mesures


def rapport(dossier):
    groupes = {}
    for fichier in sorted(dossier.glob("*/*.json")):
        donnees = json.loads(fichier.read_text())
        cle = f"{donnees['arch']}/{donnees['vcpu']}/{donnees['cas']}"
        groupes.setdefault(fichier.parent.name, {}).setdefault(cle, []).append(relever(donnees))
    if not groupes:
        raise ValueError("Aucun démarrage disponible")
    synthese = {v: {cle: statistiques(boots) for cle, boots in cellules.items()}
                for v, cellules in groupes.items()}
    appariements = {}
    for version in ("historique", "phase3"):
        if version not in groupes or "phase4" not in groupes:
            continue
        for cle in groupes[version].keys() & groupes["phase4"].keys():
            versions = [groupes[v][cle] for v in (version, "phase4")]
            par_rep = [{b["rep"]: b for b in boots} for boots in versions]
            lignes = []
            for rep in sorted(par_rep[0].keys() & par_rep[1].keys()):
                ligne = {"rep": rep}
                for horloge in ("invite_ns", "hote_ns"):
                    durees = [statistics.median(p[horloge] for p in index[rep]["lots"]) for index in par_rep]
                    ligne[horloge] = (durees[0] / durees[1] - 1) * 100
                lignes.append(ligne)
            appariements.setdefault(version, {})[cle] = lignes
    return dict(unites="7 lots après chauffe, 1 000 tours/couple. CPU hôte sur la VM entière.",
                demarrages=groupes, synthese=synthese, gains_debit_pourcent_par_demarrage=appariements)


def mesurer_version(dossier, build, arch, cpu, cas, rep):
    dossier.mkdir(parents=True, exist_ok=True)
    nom = f"{arch}-{cpu}-{cas}-{rep}"
    destination = dossier / (nom + ".json")
    elf = build.resolve() / CIBLES[arch] / "release/novaos"
    if destination.exists():
        enregistre = json.loads(destination.read_text())
        if enregistre["elf"] != hashlib.sha256(elf.read_bytes()).hexdigest():
            raise ValueError(f"Noyau différent de la mesure {destination}")
        return
    # Préserver les traces d'un essai interrompu avant la reprise.
    traces = dossier
    essai = 1
    while (traces / (nom + ".serial")).exists():
        essai += 1
        traces = dossier / "reprises" / str(essai)
        traces.mkdir(parents=True, exist_ok=True)
    donnees = mesurer(arch, cpu, cas, elf, rep, traces)
    temporaire = destination.with_suffix(".part")
    temporaire.write_text(json.dumps(donnees, separators=(",", ":")) + "\n")
    temporaire.replace(destination)
    print(destination, flush=True)


def campagne(args):
    profils = [("phase3", args.temoin), ("phase4", args.candidat)]
    if args.historique:
        profils.insert(0, ("historique", args.historique))
    # À un CPU les variantes multicouples ont les mêmes acteurs et affinités que « paires ».
    cellules = [(cpu, cas) for cpu in args.cpus for cas in args.cas
                if cpu != 1 or cas not in ("distant", "paires-cpu0", "paires-libres", "paires-sonde")]
    for rep in range(args.repetitions):
        for i, (cpu, cas) in enumerate(cellules):
            ordre = (rep + i) % len(profils)
            versions = profils[ordre:] + profils[:ordre]
            if (rep // len(profils)) % 2:
                versions.reverse()
            for arch in (args.arch if rep % 2 == 0 else reversed(args.arch)):
                for version, build in versions:
                    mesurer_version(args.sortie / version, build, arch, cpu, cas, rep)


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    sous = cli.add_subparsers(dest="action", required=True)
    preparer = sous.add_parser("construire", help="export Git et compilation du même banc pour trois noyaux")
    preparer.add_argument("sortie", type=Path)
    lancer = sous.add_parser("mesurer")
    lancer.add_argument("sortie", type=Path)
    lancer.add_argument("--temoin", required=True, type=Path)
    lancer.add_argument("--candidat", required=True, type=Path)
    lancer.add_argument("--historique", type=Path)
    lancer.add_argument("--repetitions", type=int, default=6)
    lancer.add_argument("--cpus", nargs="+", type=int, choices=(1, 2, 4), default=(1, 2, 4))
    lancer.add_argument("--arch", nargs="+", choices=CIBLES, default=list(CIBLES))
    lancer.add_argument("--cas", nargs="+", choices=CAS, default=CAS)
    rapporter = sous.add_parser("rapport")
    rapporter.add_argument("sortie", type=Path)
    rapporter.add_argument("json", type=Path)
    args = cli.parse_args()
    if args.action == "construire":
        construire(args.sortie)
    elif args.action == "mesurer":
        if args.repetitions < 1:
            cli.error("Au moins une répétition est nécessaire")
        campagne(args)
    else:
        args.json.write_text(json.dumps(rapport(args.sortie), ensure_ascii=False, separators=(",", ":")) + "\n")
