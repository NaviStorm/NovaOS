"""Observer la topologie et le montage P9 dans des exports QEMU jetables."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tempfile
from types import SimpleNamespace

from reconstruire_es_0166 import CIBLES, RACINE, empreinte, preparer_sources


def construire(revision, sortie, irq_omise, transport):
    sha = subprocess.check_output(["git", "rev-parse", "--verify", revision + "^{commit}"],
                                  cwd=RACINE, text=True).strip()
    sortie.mkdir(parents=True, exist_ok=False)
    archive = sortie / "source.tar"
    with archive.open("xb") as flux:
        subprocess.run(["git", "archive", sha, "novaOS", "components", "novaFS",
                        ":(exclude)novaOS/docs/**", ":(exclude)novaFS/docs/**"],
                       cwd=RACINE, stdout=flux, check=True)
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
    env = {**os.environ, "RUSTC": rustc, "CARGO_NET_OFFLINE": "true"}
    env.pop("CARGO_TARGET_DIR", None)
    for profil in ("brut", "coupe"):
        dossier = sortie / profil
        fixtures = preparer_sources(archive, dossier, profil)
        smp = (dossier / "novaOS/kernel/src/smp.rs").is_file()
        fixture = Path(__file__).with_name("fixtures") / (
            "stockage-0170-observation.patch" if smp else "stockage-0167-observation.patch")
        with fixture.open("rb") as flux:
            subprocess.run(["patch", "-p1", "-F0", "--batch", "--forward"], cwd=dossier,
                           check=True, stdin=flux)
        complements = []
        if transport or irq_omise:
            complements.extend(("stockage-0167-transport.patch", f"stockage-0167-collecte-{profil}.patch"))
        if irq_omise:
            complements.append("stockage-0167-irq-omise.patch")
        for nom in complements:
            injection = fixture.with_name(nom)
            with injection.open("rb") as flux:
                subprocess.run(["patch", "-p1", "-F0", "--batch", "--forward"], cwd=dossier,
                               check=True, stdin=flux)
            fixtures[injection.name] = empreinte(injection)
        noyaux = {}
        for cible in CIBLES:
            subprocess.run(["rustup", "run", "stable", "cargo", "build", "--offline", "--locked",
                            "--release", "--target", cible], cwd=dossier / "novaOS", env=env, check=True)
            noyaux[cible] = empreinte(dossier / "novaOS/target" / cible / "release/novaos")
        (dossier / "novaOS/build").mkdir()
        (dossier / "observation.json").write_text(json.dumps(dict(
            git=sha, profil=profil, fixtures=fixtures, observation=empreinte(fixture), noyaux=noyaux)))


def observer(args):
    # Les modules du banc doivent utiliser les ISO et les ELF du même export.
    sys.path.insert(0, str(args.export / "novaOS/tests"))
    from bloc import qmp
    from cli import Terminal
    import vi_volumes as volumes

    volumes.SIZES = (4 << 20,)
    volumes.HOST = args.lecteur
    os.environ["NOVA_QUALIF_VCPU"] = os.environ["NOVA_VCPUS"] = str(args.cpu)
    sortie = args.sortie
    sortie.mkdir(parents=True, exist_ok=False)
    provenance = json.loads((args.export / "observation.json").read_bytes())
    cible = CIBLES[0] if args.arch == "aarch64" else CIBLES[1]
    elf = args.export / "novaOS/target" / cible / "release/novaos"
    if empreinte(elf) != provenance["noyaux"][cible]:
        raise ValueError("Le noyau du témoin a changé depuis sa construction")
    lecteur_sha256 = empreinte(args.lecteur)
    irq_omise = "stockage-0167-irq-omise.patch" in provenance["fixtures"]
    if irq_omise and args.eio is not None:
        raise ValueError("Une seule injection par session")
    resultats = []
    with tempfile.TemporaryDirectory(prefix="nova-montage-") as nom:
        dossier = Path(nom)
        modele, _ = volumes.prepare(dossier, args.secteur)
        for i in range(args.repetitions):
            image = dossier / f"essai-{i}.raw"
            shutil.copyfile(modele, image)
            config = None
            if args.eio is not None:
                config = dossier / "eio.conf"
                # P9 parcourt les deux générations. Armer le défaut après le
                # premier passage sur ce LBA, en comptant les lectures du backend.
                etats = "".join(f'[set-state]\nevent = "read_aio"\nstate = "{n}"\n'
                                f'new_state = "{n + 1}"\n' for n in range(1, args.armer + 1))
                config.write_text(etats + f'[inject-error]\nstate = "{args.armer + 1}"\n'
                                  'event = "read_aio"\nerrno = "5"\n'
                                  f'sector = "{args.eio}"\nonce = "on"\n')
            initial = empreinte(image)
            local, distant = socket.socketpair()
            with local, distant, local.makefile("rwb", buffering=0) as flux, Terminal(
                    args.arch, args.secteur, image, sortie.name + f"-{i}", test=True,
                    qmp_bloc=distant.fileno(), defaut_bloc=config) as terminal:
                distant.close()
                mon = SimpleNamespace(stdin=flux, stdout=flux)
                qmp(mon, "qmp_capabilities")
                cpus = [v["cpu-index"] for v in qmp(mon, "query-cpus-fast")]
                # La fin négative explicite distingue le refus de montage du délai du banc.
                try:
                    terminal.until(b"nsh> ", 180, refus=b"BLOC session terminee ok=false")
                except RuntimeError:
                    if b"BLOC session terminee ok=false" not in terminal.pending:
                        raise
                    accepte = False
                else:
                    accepte = True
                    terminal.exit()
            serie = sortie / f"{i}.serial"
            shutil.copyfile(terminal.archive, serie)
            log = serie.read_text(errors="replace")
            ligne = re.search(r"BLOC bilan .*", log)
            ram = re.search(r"RAM phase=bloc_collecte .*", log)
            sondes = re.findall(r"TOPO cpu=(\d+) pid=(\d+)", log)
            if sorted(cpus) != list(range(args.cpu)) or not ligne or not ram or not sondes:
                raise RuntimeError(f"Observation incomplète dans {serie}")
            final = empreinte(image)
            paire = [(int(c), int(p)) for c, p in sondes]
            resultat = dict(essai=i, vcpu_qmp=cpus, cpu_pid=paire, invite=accepte,
                            sha256_avant=initial, sha256_apres=final, serial_sha256=empreinte(serie),
                            bilan=ligne[0], ressources=ram[0],
                            diagnostic=[s for s in log.splitlines() if "DIAG " in s or
                                        "BLOC erreur peripherique" in s or "STOCKAGE trafic" in s],
                            bloc=re.findall(r"BLOC mesure type=\w+ n=\d+", log))
            resultats.append(resultat)
            (sortie / "resultats.json").write_text(json.dumps(dict(
                arch=args.arch, secteur=args.secteur, eio_secteur_512=args.eio, armer_apres=args.armer,
                export=provenance, lecteur_sha256=lecteur_sha256, irq_omise=irq_omise,
                essais=resultats), indent=2, ensure_ascii=False) + "\n")
            # Le domaine VT-d du Q35 garde ses neuf cadres machine après détachement.
            dma_machine = 9 if args.arch == "x86_64" else 0
            compteurs = dict(re.findall(r"(\w+)=(\d+)", ram[0]))
            attendus = {"dma": dma_machine, "ipc": 0, "utilisateur": 0, "processus": 0}
            if (initial != final or "retrait=Ok(())" not in ligne[0] or
                    any(int(compteurs[k]) != v for k, v in attendus.items())):
                raise RuntimeError(f"Intégrité ou retrait en défaut : {serie}")
            if not {1, 2, 3, 4} <= {p for _, p in paire}:
                raise RuntimeError(f"Un service manque dans la trace : {serie}")
            if accepte != (args.eio is None and not irq_omise):
                raise RuntimeError(f"Montage inattendu : {serie}")
            if not accepte and ("client=Exit(26)" not in ligne[0] or "DIAG echange " not in log):
                raise RuntimeError(f"Refus non caractérisé : {serie}")
            if irq_omise and ("DIAG injection irq omise" not in log or
                              "DIAG virtio irq=Timeout" not in log or "BLOC erreur peripherique" in log):
                raise RuntimeError(f"Incident IRQ différent du témoin : {serie}")
            if not accepte:
                if any("type=write" in v or "type=flush" in v for v in resultat["bloc"]):
                    raise RuntimeError(f"Écriture ou barrière avant le refus : {serie}")
                if (irq_omise or (args.armer and args.eio == 7416)) and (
                    "DIAG echange n=2030 " not in log or
                    "BLOC mesure type=read n=2029 " not in log
                ):
                    raise RuntimeError(f"La lecture historique ciblée a changé : {serie}")
            image.unlink()


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    commandes = p.add_subparsers(dest="commande", required=True)
    c = commandes.add_parser("construire")
    c.add_argument("--irq-omise", action="store_true", help="Construire un témoin négatif distinct")
    c.add_argument("--transport", action="store_true", help="Localiser la première erreur du transport")
    c.add_argument("revision")
    c.add_argument("sortie", type=Path)
    o = commandes.add_parser("observer")
    for nom in ("export", "lecteur", "sortie"):
        o.add_argument(nom, type=lambda s: Path(s).resolve())
    o.add_argument("arch", choices=("aarch64", "x86_64"))
    o.add_argument("secteur", choices=(512, 4096), type=int)
    o.add_argument("cpu", choices=(1, 2, 4), type=int)
    o.add_argument("--repetitions", type=int, default=1)
    o.add_argument("--eio", type=int, help="LBA du backend en unités de 512 octets")
    o.add_argument("--armer", type=int, default=0, choices=(0, 1500),
                   help="Armer EIO après ce nombre de lectures QEMU")
    args = p.parse_args()
    if args.commande == "construire":
        construire(args.revision, args.sortie.resolve(), args.irq_omise, args.transport)
    else:
        if args.repetitions < 1 or (args.eio is not None and args.eio < 0):
            p.error("Nombre d'essais ou LBA invalide")
        if args.armer and args.eio is None:
            p.error("--armer demande une injection --eio")
        observer(args)
