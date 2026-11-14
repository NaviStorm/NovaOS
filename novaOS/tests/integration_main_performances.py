"""Rejouer une cellule de la comparaison classique/groupée sur un export identifié."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
import plistlib
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import time

from reconstruire_es_0166 import CIBLES, empreinte, empreinte_sources

RACINE = Path(__file__).resolve().parents[2]


def preparer(revision, racine):
    """Le profil classique ne reçoit aucune des fixtures fonctionnelles groupées."""
    sha = subprocess.check_output(["git", "rev-parse", "--verify", revision + "^{commit}"], cwd=RACINE, text=True).strip()
    racine.mkdir(parents=True, exist_ok=True)
    archive = racine / "sources.tar"
    with archive.open("xb") as flux:
        subprocess.run(["git", "archive", sha, "novaOS", "novaFS", "components",
                        ":(exclude)novaOS/docs/**", ":(exclude)novaFS/docs/**"], cwd=RACINE, stdout=flux, check=True)
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
    env = dict(os.environ, RUSTC=rustc, CARGO_NET_OFFLINE="true")
    env.pop("CARGO_TARGET_DIR", None)

    def compiler(profil):
        dossier = racine / profil
        dossier.mkdir()
        with tarfile.open(archive) as tar:
            tar.extractall(dossier, filter="data")
        noms = ["io-0166-charge-classique"] if profil != "distribution" else []
        if profil == "compteurs":
            noms += ["io-0166-sauvegarde", "io-0166-negociation-sonde"]
        preuves = {}
        for nom in noms:
            patch = RACINE / "novaOS/tests/fixtures" / (nom + ".patch")
            with patch.open("rb") as flux:
                subprocess.run(["patch", "-p1", "-F0", "--batch", "--forward", "-s"], cwd=dossier, stdin=flux, check=True)
            preuves[nom] = empreinte(patch)
        origines = empreinte_sources(dossier)
        profil_utm = {}
        binaires = {}
        builds = [(cible, "target", []) for cible in CIBLES] if profil != "distribution" else []
        builds.append((CIBLES[0], "target-utm", ["--features", "systeme-gpt"]))
        with (dossier/"compilation.log").open("x") as flux:
            for cible, target, options in builds:
                if profil == "compteurs" and options:
                    # Activer les seuls marqueurs du profil 42 sur le système GPT.
                    # Aucune implémentation de file groupée n'est liée dans cet export.
                    fichier = RACINE/"novaOS/tests/fixtures/io-0166-systeme-qualification.patch"
                    with fichier.open("rb") as patch:
                        subprocess.run(["patch", "-p1", "-F0", "--batch", "--forward", "-s"],
                                       cwd=dossier, stdin=patch, check=True)
                    profil_utm[fichier.name] = empreinte(fichier)
                subprocess.run(["rustup", "run", "stable", "cargo", "build", "--release", "--locked", "--offline",
                                "--target", cible, "--target-dir", target, *options], cwd=dossier/"novaOS", env=env,
                               check=True, stdout=flux, stderr=subprocess.STDOUT)
                path = Path("novaOS")/target/cible/"release/novaos"
                binaires[str(path)] = empreinte(dossier/path)
        (dossier/"provenance.json").write_text(json.dumps(dict(revision=sha, fixtures=preuves,
                                               sources=origines, complement_utm=profil_utm, noyaux=binaires), indent=2)+"\n")

    with ThreadPoolExecutor(max_workers=3) as pool:
        for _ in pool.map(compiler, ("distribution", "charge", "compteurs")):
            pass


def cellule(a):
    archive = a.archive.resolve()
    racine = archive/"novaOS"
    groupe = a.chemin == "groupe"
    classique = not groupe
    # L'oracle et le lanceur correspondent à l'archive choisie. Les deux bancs
    # hôtes sont ceux du dépôt qui documente les deux protocoles de FLUSH.
    sys.path.insert(0, str(racine/"tests"))
    sys.path.insert(0, str(racine/"scripts"))
    import session_cli
    session_cli.HOST = a.oracle.resolve()
    # Figer les chemins invités avant que les auxiliaires UTM ajoutent leurs
    # propres scripts hôte à sys.path. Conserver l'UART du noyau effectivement lancé.
    import cli
    if cli.ROOT != racine or session_cli.ROOT != racine or cli.HOST != a.oracle.resolve():
        raise ValueError("Le lanceur QEMU ou son lecteur ne proviennent pas de l'export sélectionné")
    import importlib.util
    def module(nom):
        spec = importlib.util.spec_from_file_location(nom, RACINE/"novaOS/tests"/(nom+".py"))
        m = importlib.util.module_from_spec(spec)
        sys.modules[nom] = m
        spec.loader.exec_module(m)
        return m
    module("nsh_utm")
    elargies = module("es_groupees_elargies")
    utm = module("es_groupees_utm")
    from qualification_es_corrigees import DESTINATIONS
    Observation = module("qos_utm").Observation
    moteur, arch = a.plateforme.split("-")
    cible = CIBLES[0] if arch == "aarch64" else CIBLES[1]
    dossier = a.sortie.resolve()
    dossier.mkdir(parents=True)
    generateur = a.generateur.resolve()
    elf = racine / ("target-utm" if moteur == "utm" and classique else "target") / cible / "release/novaos"
    provenance = json.loads((archive / "provenance.json").read_bytes())
    noyaux = provenance.get("noyaux", provenance.get("binaires", {}))
    if noyaux[str(elf.relative_to(archive))] != empreinte(elf):
        raise ValueError("Noyau différent de la provenance de l'export")
    revision = provenance.get("revision", provenance.get("revision_git"))
    preuve_livraison = None
    if a.elf_livraison or a.bilan_livraison:
        if not (a.elf_livraison and a.bilan_livraison and moteur == "utm" and classique and
                a.releve == "ordinaire" and a.cas == "inserer-milieu"):
            raise ValueError("Le témoin livré requiert un ELF système, son bilan et la cellule classique ordinaire UTM")
        elf = a.elf_livraison.resolve()
        livraison = json.loads(a.bilan_livraison.read_bytes())
        if livraison["revision_fusionnee"] != revision or livraison["binaires"]["noyau_systeme"] != empreinte(elf):
            raise ValueError("Révision ou noyau de livraison différent du relevé")
        preuve_livraison = empreinte(a.bilan_livraison)
    noyau_sha = empreinte(elf)
    sonde = a.releve == "compteurs"
    # En QEMU, la session 42 active les relevés pour le test. L'export
    # classique est matériellement dépourvu de PLAN/BATCH, quel que soit le mode.
    avant = time.time_ns()
    charge = os.getloadavg()
    if moteur == "qemu":
        (racine/"build").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="qemu-", dir=dossier) as temp:
            mesure = elargies.essai(arch, a.secteur, a.cas, generateur, Path(temp), sonde,
                                    session_classique=classique and not sonde, cpu_moteur=sys.platform == "darwin")
        serie = racine/"build"/f"cli-{arch}-{a.secteur}-elargi-{a.cas}.serial"
        (dossier/"sauvegarde.serial").write_bytes(serie.read_bytes())
    else:
        if not a.profil_utm:
            raise ValueError("Paquet modèle UTM requis")
        with tempfile.TemporaryDirectory(prefix="utm-", dir=dossier) as temp:
            machine = Path(temp)/"machine"
            try:
                cpus = plistlib.loads((a.profil_utm / "config.plist").read_bytes())["System"]["CPUCount"]
                observation = Observation("ordinaire", "repos", False, cpu_moteur=True) if cpus == 1 else None
                mesure = utm.essai(a.profil_utm.resolve(), elf, generateur, a.secteur, machine, sonde,
                                   False, a.cas, qos=observation, classique=classique)
            finally:
                for nom in ("sauvegarde.serial", "reprise.serial", "moniteur.log"):
                    serie = machine/nom
                    if serie.exists():
                        (dossier/nom).write_bytes(serie.read_bytes())
    fin = time.time_ns()
    if mesure["sha256_destination"] != DESTINATIONS[a.cas] or mesure["apres"][-1] <= mesure["avant"][-1]:
        raise ValueError("Destination ou génération invalide après la relecture indépendante")
    if moteur == "utm" and (not mesure["octets_relus"] or mesure["redemarrages"] != 1):
        raise ValueError("Relecture GPT/P9 après redémarrage absente")
    if sonde:
        c = mesure["compteur"]
        flush = c["bloc"]["flush"]["nombre"]
        if "ipc_features" not in c["controles_noyau_locaux"]:
            raise ValueError("Le compteur de négociation manque dans cette sonde")
        if groupe:
            from es_groupees_verifier_mesures import verifier_sonde
            verifier_sonde(dict(essai=dossier.name, **mesure))
            if not c.get("lots") or "attente_irq" in c:
                raise ValueError("La mesure attend la file groupée et le polling")
        if classique and ("lots" in c or flush < 2):
            raise ValueError("Chemin classique incohérent")
    serie = (dossier/"sauvegarde.serial").read_bytes()
    pids = {}
    for pid, pages, pile in re.findall(rb"CLI mem pid=(\d+) privees=(\d+) pile=(\d+)", serie):
        cle = pid.decode("ascii")
        pids[cle] = max(pids.get(cle, 0), (int(pages)+int(pile))*4)
    modele = ({nom: empreinte(a.profil_utm / nom) for nom in ("config.plist", "Data/code.fd", "Data/variables.fd")}
              if moteur == "utm" else None)
    fiche = dict(chemin=a.chemin, plateforme=a.plateforme, secteur=a.secteur, cas=a.cas,
                 releve=a.releve, debut_utc_ns=avant, fin_utc_ns=fin, charge_avant=charge, charge_apres=os.getloadavg(),
                 modele_utm=modele, provenance_sha256=empreinte(archive / "provenance.json"),
                 revision=revision, bilan_livraison_sha256=preuve_livraison,
                 noyau_sha256=noyau_sha, generateur_sha256=empreinte(generateur),
                 lecteur_sha256=empreinte(a.oracle), serial_sha256=empreinte(dossier/"sauvegarde.serial"), memoire_pid_kio=pids, mesure=mesure)
    with (dossier/"mesure.json").open("x") as flux:
        json.dump(fiche, flux, ensure_ascii=False, allow_nan=False, indent=2)
        flux.write("\n")
    print(a.plateforme, a.chemin, a.cas, a.secteur, mesure["sauvegarde_ms"], flush=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    sous = p.add_subparsers(dest="action", required=True)
    build = sous.add_parser("preparer")
    build.add_argument("revision")
    build.add_argument("racine", type=Path)
    test = sous.add_parser("mesurer")
    test.add_argument("plateforme", choices=("qemu-aarch64", "qemu-x86_64", "utm-aarch64"))
    test.add_argument("chemin", choices=("classique", "groupe"))
    test.add_argument("releve", choices=("ordinaire", "compteurs"))
    test.add_argument("secteur", type=int, choices=(512,4096))
    test.add_argument("cas", choices=("neuf-4m", "inserer-milieu", "ajout-4m-multiple"))
    for champ in ("archive", "generateur", "oracle", "sortie"):
        test.add_argument(champ, type=Path)
    test.add_argument("--profil-utm", type=Path)
    test.add_argument("--elf-livraison", type=Path)
    test.add_argument("--bilan-livraison", type=Path)
    args = p.parse_args()
    if args.action == "preparer":
        preparer(args.revision, args.racine.resolve())
    else:
        cellule(args)
