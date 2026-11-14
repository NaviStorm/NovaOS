"""Construire ou analyser les sondes détaillées de phase 4 sur une copie Git fraîche."""
import argparse
import collections
import json
import math
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import tarfile

from ipc_multicoeur import CIBLES

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = Path(__file__).resolve().parent / "fixtures"
COMMUN = "ipc-phase4-sites-communs.patch"
P3 = "ipc-phase4-sites-p3-p4.patch"
PROFILS = {
    "historique": ("f356970d27cf8e8fad24d487a3debdc4d2a0be18", [COMMUN, "ipc-phase4-sites-historique.patch"]),
    "temoin": ("4e741fbb2e40df84f69f1a2e8ee40b5182968650", [COMMUN, P3, "ipc-phase4-sites-temoin.patch"]),
    "candidat": ("a8dc2534ac9fabcca2e480d1054fd0b60f180f17", [COMMUN, P3, "ipc-phase4-sites-candidat.patch"]),
}
LIGNE = re.compile(r"^P4 site=(\w+) lot=(\d+) groupe=(\d+) appels=(\d+) ticks=(\d+)$", re.M)
RAM = re.compile(r"^P4 ram lot=(\d+) hits=(\d+) recherches=(\d+)$", re.M)
SITES = (
    {"capacite", "droits", "offre", "receveur", "publication", "attente", "signaux", "collecte", "tables_locales"},
    {"translation", "region_ram", "copie_depuis", "copie_vers", "charge_utile", "traitement"},
    {"identite", "choix", "reveil", "bascule", "espace", "ipi_emission", "irq_service", "tlb"},
)


def construire(args):
    revision_defaut, patches = PROFILS[args.profil]
    revision = subprocess.check_output(["git", "rev-parse", "--verify", "--end-of-options",
                                        (args.revision or revision_defaut) + "^{commit}"],
                                       cwd=ROOT, text=True).strip()
    dest = args.sortie.resolve()
    dest.mkdir(parents=True, exist_ok=False)
    source = dest / "src"
    source.mkdir()
    with subprocess.Popen(["git", "archive", revision, "novaOS", "novaFS", "components",
                           ":(exclude)novaOS/docs", ":(exclude)novaFS/docs"], cwd=ROOT,
                          stdout=subprocess.PIPE) as archive:
        with tarfile.open(mode="r|", fileobj=archive.stdout) as contenu:
            contenu.extractall(source, filter="data")
        if archive.wait():
            raise RuntimeError(f"Archive Git incomplète pour {revision}")
    shutil.copyfile(FIXTURES / "ipc-phase4-sites.rs", source / "novaOS/kernel/src/profil4.rs")
    patches = list(patches)
    if args.compter_ram:
        if args.profil == "historique":
            raise ValueError("Comptage RAM prévu sur le témoin et le candidat de phase 4")
        patches += ["ipc-phase4-sites-compte.patch",
                    "ipc-phase4-ram.patch" if args.profil == "candidat" else "ipc-phase4-ram-temoin.patch"]
    for patch in patches:
        with (FIXTURES / patch).open("rb") as flux:
            subprocess.run(["patch", "-s", "--batch", "-F", "0", "-p1"], stdin=flux, cwd=source, check=True)
    env = os.environ | {"RUSTC": subprocess.check_output(
        ["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()}
    for arch in args.arch:
        subprocess.run(["rustup", "run", "stable", "cargo", "build", "--offline", "--locked",
                        "--release", "--features", "ipc-smp-lab", "--target", CIBLES[arch],
                        "--target-dir", dest / "elf"], cwd=source / "novaOS", env=env, check=True)


def quantiles(valeurs):
    v = sorted(valeurs)
    def rang(fraction):
        return v[math.ceil(fraction * len(v)) - 1]
    return dict(n=len(v), p50=statistics.median(v), p95=rang(.95), p99=rang(.99),
                iqr=rang(.75) - rang(.25), min=v[0], max=v[-1])


def analyser(args):
    campagnes = {}
    for dossier in args.campagne:
        demarrages = []
        for fichier in sorted(dossier.rglob("*.json")):
            vm = json.loads(fichier.read_text())
            if vm["cas"] != "paires-sonde":
                raise ValueError(f"Scénario inattendu dans {fichier}")
            trace = fichier.with_suffix(".serial").read_text()
            sites = collections.defaultdict(dict)
            for site, lot, groupe, appels, ticks in LIGNE.findall(trace):
                lot, groupe = int(lot), int(groupe)
                assert site not in sites[lot] and groupe == (max(0, lot - 1) % 3) + 1
                sites[lot][site] = [int(appels), int(ticks)]
            ram = {int(lot): [int(hits), int(recherches)] for lot, hits, recherches in RAM.findall(trace)}
            assert len(sites) == 8
            mesures = []
            for lot, valeurs in enumerate(vm["releves"]):
                famille = max(0, lot - 1) % 3
                assert set(sites[lot]) == SITES[famille] | {"vide"}
                assert sites[lot]["vide"][0] == 128
                assert len(valeurs["couples"]) == vm["vcpu"] and max(valeurs["couples"]) == valeurs["ns"]
                if lot == 0:
                    continue
                old = vm["releves"][lot-1]["cpu"]
                compteurs = {k: sum(c[k]-o[k] for c, o in zip(valeurs["cpu"], old))
                             for k in old[0] if k not in ("cpu", "max")}
                item = dict(lot=lot, sites=sites[lot], invite_ns=valeurs["ns"], hote_ns=valeurs["hote_ns"],
                            couples_ns=valeurs["couples"], profil=compteurs)
                if lot in ram:
                    assert sum(ram[lot]) == sites[lot]["region_ram"][0]
                    item["ram_hits_recherches"] = ram[lot]
                mesures.append(item)
            demarrages.append(dict(profil=fichier.parent.name, architecture=vm["arch"], vcpu=vm["vcpu"],
                                   repetition=vm["repetition"], elf=vm["elf"], hz=vm["config"]["hz"],
                                   charge=vm["charge_fin"], cpu_hote_user_s=vm["cpu_hote_user_s"],
                                   cpu_hote_sys_s=vm["cpu_hote_sys_s"], lots=mesures))
        if not demarrages:
            raise ValueError(f"Aucune trace instrumentée dans {dossier}")
        groupes = collections.defaultdict(lambda: collections.defaultdict(list))
        for b in demarrages:
            cle = f"{b['profil']}/{b['architecture']}/{b['vcpu']}"
            for lot in b["lots"]:
                for site, (n, t) in lot["sites"].items():
                    groupes[cle][site].append((n, t * 1000 / b["hz"]))
        synthese = {cle: {site: {"appels": quantiles([n for n, _ in vals]),
                                "ms": quantiles([ms for _, ms in vals])}
                          for site, vals in sites.items()} for cle, sites in groupes.items()}
        campagnes[dossier.name] = dict(demarrages=demarrages, synthese=synthese)
    with args.sortie.open("x") as sortie:
        # Une ligne par démarrage pour comparer et relire indépendamment les valeurs.
        sortie.write("{\n")
        for i, (nom, campagne) in enumerate(campagnes.items()):
            if i:
                sortie.write(",\n")
            sortie.write("  " + json.dumps(nom) + ': {"demarrages": [\n')
            for j, boot in enumerate(campagne["demarrages"]):
                if j:
                    sortie.write(",\n")
                sortie.write("    " + json.dumps(boot, separators=(",", ":")))
            sortie.write('\n  ], "synthese": {\n')
            for j, (cle, valeurs) in enumerate(campagne["synthese"].items()):
                if j:
                    sortie.write(",\n")
                sortie.write("    " + json.dumps(cle) + ": " + json.dumps(valeurs, separators=(",", ":")))
            sortie.write("\n  }}")
        sortie.write("\n}\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commandes = parser.add_subparsers(required=True)
    build = commandes.add_parser("construire", help="préparer une copie isolée et construire le profil")
    build.add_argument("profil", choices=PROFILS)
    build.add_argument("sortie", type=Path, help="nouveau répertoire de construction")
    build.add_argument("--arch", nargs="+", choices=CIBLES, default=list(CIBLES))
    build.add_argument("--revision", help="révision alternative au schéma de sources compatible")
    build.add_argument("--compter-ram", action="store_true")
    build.set_defaults(action=construire)
    synth = commandes.add_parser("analyser", help="vérifier et convertir les couples de fichiers JSON/série")
    synth.add_argument("sortie", type=Path)
    synth.add_argument("campagne", type=Path, nargs="+")
    synth.set_defaults(action=analyser)
    arguments = parser.parse_args()
    arguments.action(arguments)
