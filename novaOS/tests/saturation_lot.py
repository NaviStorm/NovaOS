"""Saturation d'une réservation SG sur un export jetable, avec lecture retenue dans QEMU."""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
from types import SimpleNamespace

from reconstruire_es_0166 import CIBLES, RACINE, empreinte, preparer_sources


def instrumenter(archive):
    noyau = archive / "novaOS/kernel/src"
    ipc = noyau / "sched/process/ipc"
    source = Path(__file__).with_name("saturation_lot_invite.rs")
    (ipc / "probes/saturation.rs").write_bytes(source.read_bytes())
    with (ipc / "probes.rs").open("a") as fichier:
        fichier.write("\nmod saturation;\npub(crate) use saturation::{apres_kick, exercice};\n")
    with (ipc / "mod.rs").open("a") as fichier:
        fichier.write("\npub(crate) use probes::{apres_kick, exercice};\n")

    def inserer(fichier, repere, texte):
        contenu = fichier.read_text()
        if contenu.count(repere) != 1:
            raise ValueError(f"Point de synchronisation absent ou ambigu dans {fichier}")
        fichier.write_text(contenu.replace(repere, repere + texte))

    inserer(noyau / "hardware/block/arena.rs", "self.pending = Some(seen);\n        drop(irq);",
            "\n        crate::sched::process::ipc::apres_kick();")
    inserer(noyau / "main.rs", "    let scenario = args.as_slice();",
            '\n    if scenario == b"cli-test" { sched::process::ipc::exercice(); hal::cpu::halt() }')


def executer(arch, secteur, dossier):
    from bloc import qmp
    from cli import Terminal
    dossier.mkdir()
    image = dossier / "lecture.raw"
    original = bytearray(1 << 20)
    original[8192:24576] = bytes(0x71 + i % 83 for i in range(16384))
    image.write_bytes(original)
    empreinte_initiale = empreinte(image)
    essais = []
    with ExitStack() as pile:
        local, distant = socket.socketpair()
        pile.enter_context(local)
        pile.enter_context(distant)
        flux = pile.enter_context(local.makefile("rwb", buffering=0))
        moniteur = SimpleNamespace(stdin=flux, stdout=flux)
        terminal = pile.enter_context(Terminal(arch, secteur, image, "saturation-lot", test=True,
                                               qmp_bloc=distant.fileno()))
        distant.close()
        qmp(moniteur, "qmp_capabilities")

        def attendre(marque):
            return terminal.until(marque, refus=b"#PF adresse=")

        def controle(commande):
            retour = qmp(moniteur, "human-monitor-command",
                         **{"command-line": f'qemu-io d "{commande}"'})
            if retour != "":
                raise RuntimeError(f"{commande}: {retour}")

        def compteurs():
            entrees = qmp(moniteur, "query-blockstats")
            stats = next(e["stats"] for e in entrees if e["device"] == "d")
            return tuple(stats[nom] for nom in ("rd_operations", "wr_operations", "flush_operations"))

        for retrait in ("false", "true"):
            attendre(f"SATURATION armer retrait={retrait}".encode())
            etiquette = "saturation-" + retrait
            controle("break read_aio " + etiquette)
            avant = compteurs()
            terminal.vm.stdin.write(b"A")
            attendre(f"SATURATION kick retrait={retrait}".encode())
            controle("wait_break " + etiquette)
            terminal.vm.stdin.write(b"B")
            attendre(f"SATURATION nouveau tag refuse retrait={retrait}".encode())
            pendant = compteurs()
            assert avant == pendant, (avant, pendant)
            controle("resume " + etiquette)
            terminal.vm.stdin.write(b"C")
            preuve = f"SATURATION reprise exacte retrait={retrait} kicks=2 secteur={secteur}"
            attendre(preuve.encode())
            apres = compteurs()
            assert apres == (avant[0] + 2, avant[1], avant[2]), (avant, apres)
            essais.append(dict(retrait=retrait == "true", evenement_suspendu="read_aio",
                               lectures_terminees=[avant[0], pendant[0], apres[0]], ecritures=0, flush=0,
                               kicks=2, preuve=preuve))
        attendre(b"SATURATION deux lots qualifies")
    assert image.read_bytes() == original
    return dict(arch=arch, secteur=secteur, essais=essais, sha256_image_avant=empreinte_initiale,
                sha256_image_apres=empreinte(image), sha256_serie=empreinte(terminal.archive))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("revision")
    p.add_argument("sortie", type=Path, help="répertoire neuf de l'archive et des preuves")
    args = p.parse_args()
    dossier = args.sortie.resolve()
    dossier.mkdir(parents=True, exist_ok=False)
    revision = subprocess.check_output(["git", "rev-parse", "--verify", args.revision + "^{commit}"],
                                      text=True, cwd=RACINE).strip()
    tar = dossier / "source.tar"
    with tar.open("xb") as f:
        subprocess.run(["git", "archive", revision, "novaOS", "novaFS", "components",
                        ":(exclude)novaOS/docs/**", ":(exclude)novaFS/docs/**"],
                       cwd=RACINE, stdout=f, check=True)
    archive = dossier / "invite"
    provenance = preparer_sources(tar, archive, "sans")
    instrumenter(archive)
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
    env = dict(os.environ, RUSTC=rustc, CARGO_NET_OFFLINE="true")
    env.pop("CARGO_TARGET_DIR", None)
    sys.path.insert(0, str(archive / "novaOS/tests"))
    rapport = dict(revision=revision, profil="sans + sonde de saturation", fixtures=provenance,
                   sha256_sonde=empreinte(Path(__file__).with_name("saturation_lot_invite.rs")),
                   sha256_banc=empreinte(Path(__file__)), elfs={}, cellules=[], erreurs=[])
    fichier = dossier / "validation.json"

    def sauvegarder():
        temporaire = dossier / "validation.partielle"
        temporaire.write_text(json.dumps(rapport, ensure_ascii=False, indent=2) + "\n")
        temporaire.replace(fichier)

    for arch, cible in zip(("aarch64", "x86_64"), CIBLES):
        with (dossier / (arch + ".compilation.log")).open("x") as log:
            commande = ["rustup", "run", "stable", "cargo", "build", "--locked", "--offline",
                        "--release", "--target", cible]
            compilation = subprocess.run(commande, cwd=archive / "novaOS", env=env,
                                         stdout=log, stderr=subprocess.STDOUT)
        if compilation.returncode:
            rapport["erreurs"].append(dict(arch=arch, etape="compilation", code=compilation.returncode))
            sauvegarder()
            continue
        rapport["elfs"][arch] = empreinte(archive / "novaOS/target" / cible / "release/novaos")
        for secteur in (512, 4096):
            try:
                cellule = executer(arch, secteur, dossier / f"{arch}-{secteur}")
            except (AssertionError, OSError, RuntimeError, subprocess.SubprocessError) as exc:
                cellule = dict(arch=arch, secteur=secteur, type=type(exc).__name__, erreur=str(exc))
                rapport["erreurs"].append(cellule)
            else:
                rapport["cellules"].append(cellule)
            sauvegarder()
            print(json.dumps(cellule, ensure_ascii=False), flush=True)
    if rapport["erreurs"]:
        raise SystemExit(f"Cellules en échec, voir {fichier}")


if __name__ == "__main__":
    main()
