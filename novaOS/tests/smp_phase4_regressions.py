"""Rejouer une cellule fonctionnelle SMP dans un répertoire privé, sur des ELF identifiés."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


TARGETS = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def prepare(source, output):
    cell = output / "novaOS"
    output.mkdir(parents=True, exist_ok=True)
    if cell.exists() and (output / "novaFS").resolve(strict=True) != (source / "novaFS").resolve(strict=True):
        raise ValueError("Cette cellule appartient à un autre export.")
    if not cell.exists():
        cell.mkdir()
        for directory in ("tests", "scripts", "boot", "licenses"):
            shutil.copytree(source / "novaOS" / directory, cell / directory,
                            ignore=shutil.ignore_patterns("__pycache__"))
        for name in ("target", "third_party", "user", "LICENSE"):
            (cell / name).symlink_to(source / "novaOS" / name)
        for name in ("components", "novaFS"):
            (output / name).symlink_to(source / name)
        (cell / "build").mkdir()
    return cell


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="export Git avec le noyau ordinaire et novafs hôte construits")
    parser.add_argument("sortie", type=Path, help="dossier privé de cette seule architecture/topologie")
    parser.add_argument("architecture", choices=TARGETS)
    parser.add_argument("vcpu", type=int, choices=(1, 2, 4))
    parser.add_argument("--noyau", type=Path, help="ELF ordinaire pour le rejeu d’un correctif identifié")
    parser.add_argument("--fp", type=Path, required=True, help="ELF user-fpu")
    parser.add_argument("--gic3", type=Path, help="ELF AArch64 gicv3-lab,user-fpu")
    parser.add_argument("--cas", nargs="+", help="sélectionner des tests par leur nom")
    args = parser.parse_args()
    source, out = args.source.resolve(), args.sortie.resolve()
    arch, n = args.architecture, args.vcpu
    work = prepare(source, out)
    target = TARGETS[arch]
    elf = (args.noyau or source / "novaOS/target" / target / "release/novaos").resolve(strict=True)
    fp = args.fp.resolve(strict=True)
    gic = args.gic3.resolve(strict=True) if args.gic3 else None
    env = dict(os.environ, NOVA_ARCH=arch, NOVA_VCPUS=str(n), NOVA_QUALIF_VCPU=str(n),
               NOVA_KERNEL_ELF=str(elf), PYTHONPATH=str(work / "tests"))
    for key in ("NOVA_GIC", "NOVA_X86_CPU", "NOVA_ARM_CPU", "NOVA_ISO", "NOVA_RAM", "NOVA_VGA"):
        env.pop(key, None)
    python = [sys.executable, "-B"]
    cases = []

    def command(name, argv, **settings):
        cases.append((name, argv, env | settings))

    def suite(name, script, *params):
        command(name, python + ["tests/" + script + ".py", *params])

    def smp(name, **settings):
        command(name, python + ["-c", f"import smp; smp.run({arch!r}, {n})"], **settings)

    def stop(name, scenario, **settings):
        expr = f"import smp_phase2; smp_phase2.run({arch!r}, {scenario!r}, {n})"
        command(name, python + ["-c", expr], **settings)

    def fp_control(name, **settings):
        expr = f"import demarrage; demarrage.process_test({arch!r},user_fp=True)"
        command(name, python + ["-c", expr], **settings)

    command("iso", ["sh", "scripts/iso.sh"])
    smp("smp")
    for scenario in ["smp-arret", "smp-panique"] + (["smp-ap-absent", "smp-cpu-muet"] if n > 1 else []):
        stop(scenario, scenario)
    if arch == "aarch64":
        stop("fp-neon-gic2", "smp-fp", NOVA_KERNEL_ELF=str(fp))
        fp_control("processus-neon", NOVA_KERNEL_ELF=str(fp))
        if gic:
            settings = dict(NOVA_KERNEL_ELF=str(gic), NOVA_GIC="3")
            smp("smp-gic3", **settings)
            stop("fp-neon-gic3", "smp-fp", **settings)
    else:
        for label, cpu in (("avx", "max,vendor=GenuineIntel"),
                           ("sans-avx", "qemu64,vendor=GenuineIntel,-avx,-xsave")):
            settings = dict(NOVA_KERNEL_ELF=str(fp), NOVA_X86_CPU=cpu)
            stop("fp-" + label, "smp-fp", **settings)
            fp_control("processus-" + label, **settings)
    suite("ipc", "ipc", "test", "--ram", "128")
    suite("courses", "ipc", "races")
    suite("elf", "elf")
    suite("cycle-processus", "cycle_processus")
    suite("boot", "demarrage")
    for name in ("bloc", "namespace", "cli", "cli_p9", "nsh", "nsh_scripts", "nsh_terminal",
                 "vi", "vi_ecran", "fichiers"):
        suite(name, name)
    if n == 4:
        suite("endurance-ipc", "ipc", "endurance", "--seconds", "300")
        suite("endurance-ordonnanceur", "demarrage", "--endurance", "60")
        suite("ipc-t17", "ipc", "perf")
    selected = set(args.cas or [c[0] for c in cases])
    missing = selected - {c[0] for c in cases}
    if missing:
        parser.error("cas inconnus pour ce profil : " + ", ".join(sorted(missing)))
    # Le démarrage classique utilise l'ISO de distribution créée dans cette cellule.
    if "boot" in selected:
        selected.add("iso")
    failed = []
    for name, argv, settings in cases:
        if name not in selected:
            continue
        # Chaque essai, y compris un rejeu, conserve sa sortie et son code retour.
        trial = 1
        while (out / f"{name}-{trial}.log").exists():
            trial += 1
        log = out / f"{name}-{trial}.log"
        ident = digest(Path(settings["NOVA_KERNEL_ELF"]))
        print(f"{arch}/{n} {name} essai {trial}", flush=True)
        start = time.monotonic()
        with log.open("xb") as stream:
            result = subprocess.run(argv, cwd=work, env=settings, stdout=stream, stderr=subprocess.STDOUT)
        row = dict(architecture=arch, vcpu=n, nom=name, essai=trial, secondes=round(time.monotonic()-start, 3),
                   retour=result.returncode, elf_sha256=ident, log=log.name, log_sha256=digest(log))
        (log.with_suffix(".json")).write_text(json.dumps(row, ensure_ascii=False, indent=2) + "\n")
        if result.returncode:
            failed.append(name)
    if failed:
        raise SystemExit("Échecs : " + ", ".join(failed))


if __name__ == "__main__":
    main()
