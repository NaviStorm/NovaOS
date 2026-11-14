"""Conservation des preuves des seuls volumes fictifs d'une qualification stockage."""
import hashlib
import json
from contextlib import ExitStack, contextmanager
from datetime import datetime, timezone
from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile


def horodatage():
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def enregistrer(dossier, nom, donnees):
    with (dossier / nom).open("x", encoding="utf-8") as sortie:
        json.dump(donnees, sortie, ensure_ascii=False, indent=2)
        sortie.write("\n")


def empreinte(chemin):
    with chemin.open("rb") as flux:
        return hashlib.file_digest(flux, "sha256").hexdigest()


def provenance(racine):
    export = racine / "provenance.json"
    if export.is_file():
        fiche = json.loads(export.read_bytes())
        return dict(revision_git=fiche.get("revision_git", fiche.get("revision")),
                    profil=fiche.get("profil"), provenance_sha256=empreinte(export))
    if (racine / ".git").exists():
        sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=racine, text=True).strip()
        return dict(revision_git=sha)
    return dict(revision_git=None)


def copier_image(image, nom):
    destination = image.parent / nom
    with image.open("rb") as source, destination.open("xb") as cible:
        shutil.copyfileobj(source, cible)
    return destination


def figer_initiale(image):
    """Prendre la copie P9 après préparation et avant le lancement de QEMU."""
    initiale = copier_image(image, "initiale.raw")
    initiale.chmod(0o400)
    sha = empreinte(initiale)
    if sha != empreinte(image):
        raise ValueError("La copie initiale diffère du volume fictif")
    enregistrer(image.parent, "images.json",
                dict(initiale=initiale.name, apres_defaillance=image.name, initiale_sha256=sha))
    return initiale


def copie_reprise(image):
    """QEMU a quitté. Garder le disque arrêté intact et redémarrer sa copie."""
    image.chmod(0o400)
    return copier_image(image, "reprise.raw")


def configuration_vm(journal, cmd, env, arch, secteur):
    # La commande de bloc passe par qemu.sh. Sa dernière ligne « exec » étendue
    # est enregistrée dans le journal QEMU ; ne jamais copier l'environnement.
    iso = Path(env["NOVA_ISO"])
    racine = Path(cmd[1]).resolve().parents[2]
    cible = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}[arch]
    elf = Path(env.get("NOVA_KERNEL_ELF", racine / "novaOS/target" / cible / "release/novaos"))
    if "NOVA_KERNEL_ELF" in env:
        # Un ELF injecté peut appartenir à un autre commit que le lanceur.
        racine_elf = next((p for p in elf.resolve().parents
                           if (p / "provenance.json").is_file() or (p / ".git").exists()), None)
    else:
        racine_elf = racine
    enregistrer(journal.parent, journal.stem + ".vm.json", dict(
        debut_utc=horodatage(), architecture=arch, octets_secteur=secteur,
        vcpu=int(env.get("NOVA_VCPUS", "1")), ram_mio=int(env.get("NOVA_RAM", "256")),
        iso_sha256=empreinte(iso), elf_sha256=empreinte(elf) if elf.is_file() else None,
        lanceur=provenance(racine), construction=provenance(racine_elf) if racine_elf else None,
        commande=[str(arg) for arg in cmd]))


CATEGORIES = {
    "ipc": re.compile("FS0170 transport|BLOC0170 tag=|DIAG appel|IPC|SURETE0170|TAG0170|RELAIS0170|PHASE.*(?:ipc|transport)"),
    "stockage": re.compile("FS0170|BLOC0170|BLOC (?:erreur|bilan)|FICHIERS|PHASE|COUPURE groupe|NVFS|VFS|DMA qualif|SURETE0170|P9|INJECT"),
}

PUBLIC = {"erreur d’E/S": 26, "publication incertaine": 22, "fichier enregistré": 0}


def extraire_traces(serie):
    # Les séries restent l'autorité. Un point de coupure peut précéder tout statut.
    statut = []
    compte = dict.fromkeys(CATEGORIES, 0)
    with ExitStack() as pile:
        source = pile.enter_context(serie.open("rb"))
        sorties = {cle: pile.enter_context(serie.with_suffix("." + cle + ".log").open("x"))
                   for cle in CATEGORIES}
        for brute in source:
            ligne = brute.decode("utf-8", errors="replace")
            for cle, motif in CATEGORIES.items():
                if motif.search(ligne):
                    sorties[cle].write(ligne)
                    compte[cle] += 1
            for texte, code in PUBLIC.items():
                if texte in ligne:
                    statut.append(dict(message=texte, code=code))
            # En mode files-inject (24), le client est lui-même l'application
            # de test. Le code de sortie du shell en mode 42 qualifie sa session.
            retour = re.search(r"BLOC bilan mode=24 .*?client=Exit\((\d+)\)", ligne)
            if retour:
                statut.append(dict(message=retour.group(0), code=int(retour[1])))
    return dict(traces_lignes=compte, messages_publics_observes=statut)


@contextmanager
def dossier_essai(rapport, arch, secteur, point, *, conserver=False, parametres=None):
    # Le nom neuf et le mode 0700 de mkdtemp isolent les passages concurrents.
    dossier = Path(tempfile.mkdtemp(dir=Path(rapport).resolve().parent,
                                    prefix=f"nova-cow-{arch}-{secteur}-{point}-"))
    erreur = None
    try:
        enregistrer(dossier, "debut.json", dict(
            debut_utc=horodatage(), architecture=arch, secteur=secteur,
            vcpu_demande=int(os.environ.get("NOVA_VCPUS", "1")), injection=point,
            parametres=parametres or {}, banc=provenance(Path(__file__).resolve().parents[2])))
        yield dossier
    except BaseException as exc:
        erreur = exc
        exc.add_note(f"Preuves du passage conservées dans {dossier}")
        raise
    finally:
        try:
            if erreur is not None or conserver:
                # Les remontages ultérieurs se font sur des copies inscriptibles.
                for motif in ("*.raw", "*.qcow2"):
                    for image in dossier.glob(motif):
                        if image.is_file() and not image.is_symlink():
                            image.chmod(0o400)
            traces = {p.name: extraire_traces(p) for p in sorted(dossier.glob("*.serial"))}
            fichiers = {f.name: dict(octets=f.stat().st_size, sha256=empreinte(f))
                        for f in sorted(dossier.iterdir()) if f.is_file() and not f.is_symlink()}
            enregistrer(dossier, "bilan.json", dict(
                fin_utc=horodatage(), resultat="echec" if erreur else "succes",
                exception=repr(erreur) if erreur else None, fichiers=fichiers, series=traces))
        except Exception as collecte:
            if erreur is None:
                collecte.add_note(f"Collecte incomplète ; fichiers conservés dans {dossier}")
                raise
            erreur.add_note(f"Collecte incomplète dans {dossier} : {collecte!r}")
        else:
            if erreur is None and not conserver and not any(dossier.glob("nova-cow-*")):
                shutil.rmtree(dossier)


def nettoyer_apres_validation(dossier, confirmation):
    """Supprimer un passage achevé dont l'opérateur a revu et confirmé le manifeste."""
    if dossier.is_symlink() or not dossier.is_dir():
        raise ValueError("Un répertoire d'essai est requis")
    debut = json.loads((dossier / "debut.json").read_bytes())
    prefixe = f"nova-cow-{debut['architecture']}-{debut['secteur']}-{debut['injection']}-"
    if not dossier.name.startswith(prefixe):
        raise ValueError("Ce répertoire ne correspond pas au passage enregistré")
    chemin = dossier / "bilan.json"
    if chemin.is_symlink() or empreinte(chemin) != confirmation:
        raise ValueError("L'empreinte explicitement confirmée doit correspondre au bilan")
    bilan = json.loads(chemin.read_bytes())
    attendus = set(bilan["fichiers"]) | {chemin.name}
    if {p.name for p in dossier.iterdir()} != attendus:
        raise ValueError("Inventaire incomplet ou contenu ajouté depuis la collecte")
    for nom, preuve in bilan["fichiers"].items():
        p = dossier / nom
        if p.is_symlink() or not p.is_file() or empreinte(p) != preuve["sha256"]:
            raise ValueError(f"Élément modifié depuis la revue : {nom}")
    shutil.rmtree(dossier)


if __name__ == "__main__":
    import argparse
    p = argparse.ArgumentParser(description="Nettoyage manuel d'un seul dossier de preuves revu")
    p.add_argument("dossier", type=Path)
    p.add_argument("--confirmer-bilan-sha256", required=True,
                   help="SHA-256 du bilan.json après revue humaine du passage et de ses images")
    args = p.parse_args()
    nettoyer_apres_validation(args.dossier, args.confirmer_bilan_sha256)
