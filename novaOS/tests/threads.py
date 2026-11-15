"""Compile le témoin natif hors du monorepo, l'admet, puis l'exécute en TCG."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tarfile
import tempfile
import time

from demarrage import ROOT, BUILD, scenario_vm


CARGO = ["rustup", "run", "stable", "cargo"]
TARGETS = {"x86_64": "x86_64-unknown-none", "aarch64": "aarch64-unknown-none-softfloat"}


def build_application(work, arch, env, example):
    if work.is_relative_to(ROOT.parent):
        raise ValueError("la construction du témoin exige un workspace externe")
    pack = ROOT.parent / "distribution/scripts/sdk_rust.py"
    archive = Path(subprocess.check_output([sys.executable, pack, "--check"], text=True).strip())
    with tarfile.open(archive, "r:gz") as bundle:
        bundle.extractall(work, filter="data")
    sdk = work / archive.name.removesuffix(".tar.gz")
    project = sdk / "examples/threads"
    metadata = json.loads(subprocess.check_output(
        [*CARGO, "metadata", "--locked", "--offline", "--format-version", "1"],
        cwd=project, env=env))
    packages = {p["name"]: Path(p["manifest_path"]).resolve() for p in metadata["packages"]}
    assert packages == {"temoin": project / "Cargo.toml", "nova-system": sdk / "libs/nova-system/Cargo.toml"}
    target = TARGETS[arch]
    args = [sys.executable, sdk / "build.py", arch]
    if not example:
        args.extend(["--features", "qualification"])
    subprocess.run(args, cwd=project, check=True, env={
        **env, "CARGO_TARGET_DIR": str(work / "application-target"),
    })
    elf = work / "application-target" / target / "release/temoin"
    data = elf.read_bytes()
    kind, machine = struct.unpack_from("<HH", data, 16)
    assert data[:7] == b"\x7fELF\x02\x01\x01" and (kind, machine) == (2, 62 if arch == "x86_64" else 183)
    start = struct.unpack_from("<Q", data, 32)[0]
    stride, count = struct.unpack_from("<HH", data, 54)
    types = [struct.unpack_from("<I", data, start + i * stride)[0] for i in range(count)]
    assert 1 in types and not set(types).intersection((2, 3, 7))
    return elf, {"cible": target, "application": str(project), "artefact": str(elf),
                 "sha256": hashlib.sha256(data).hexdigest(), "octets": len(data),
                 "segments_elf": types, "sdk": archive.name,
                 "sdk_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
                 "exemple_sans_sonde": example}


def exercise(arch, cpus, suffix):
    def check(vm, directory, log):
        deadline = time.monotonic() + 75
        text = ""
        while vm.poll() is None and time.monotonic() < deadline:
            text = log.read_text(errors="replace") if log.exists() else ""
            if re.search(r"THREAD restitution [^\r\n]* OK\r?\n", text):
                break
            if "Panique du noyau" in text:
                raise RuntimeError(text[-1800:])
            time.sleep(.05)
        markers = ["externe somme=432 threads=2 tls=7,11 double-join=refuse",
                   "tas aligne stable RTC UTC ouverture et vues fichier repertoire refusees",
                   "IPC reception retiree BAD_ADDRESS puis message=37",
                   "admission absente creation attente TLS refusees ID et yield OK",
                   "cycle annulation delai detachement quota=8 reemploi-tls=0",
                   "capacite delegation envoi puis revocation racine conservee",
                   "THREADS VM gardes=2 tls=zero recyclage=verifie rollback=verifie",
                   "notifications 1+1+0 concurrence valeur-publiee=29",
                   "FPU SIMD GPR TLS conserves sur deux threads",
                   "THREAD principal collecte dernier resultat=19",
                   "auto-join refuse attenteur unique join annule cible collectee ancien handle refuse",
                   "SDK A annule transfere le handle de B a C collecte unique double join refuse",
                   "attente et join annules", "THREAD sorties concurrentes=2 dernier=",
                   "THREAD garde adresse=", "faute=confirmee",
                   "THREAD destruction attentes=2 ipc=1 calcul=1 join=1 survivants=0"]
        if not all(marker in text for marker in markers):
            raise RuntimeError(f"témoignage utilisateur incomplet\n{text[-1800:]}")
        progress = re.search(r"THREAD calcul cpu=0->(\d+) preemptions=(\d+),(\d+) migrations=(\d+) isolation=OK", text)
        freed = re.search(r"THREAD restitution avant=(\d+) apres=(\d+) blocs=(\d+) tcb=(\d+) OK", text)
        assert progress and freed, text[-1500:]
        dest, a, b, moved = map(int, progress.groups())
        before, after, blocks, slots = map(int, freed.groups())
        assert dest == cpus - 1 and min(a, b) >= 2 and moved == (2 if cpus > 1 else 0)
        assert before == after and slots == cpus + 1
        level = "EL0" if arch == "aarch64" else "Ring3"
        assert re.search(rf"niveau={level} .* sortie=0", text)
        return {"vcpus": cpus, "niveau": level, "cpu_final": dest, "preemptions": [a, b],
                "migrations": moved, "cadres_avant": before, "cadres_apres": after,
                "blocs": blocks, "tcb_apres": slots}
    return scenario_vm(arch, "threads-test", check, f"-{cpus}cpu{suffix}", NOVA_VCPUS=str(cpus))


def exercise_example(arch, cpus):
    def check(vm, directory, log):
        marker = "SDK exemple admis sortie=0 restitution=OK"
        level = "EL0" if arch == "aarch64" else "Ring3"
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            output = log.read_text(errors="replace") if log.exists() else ""
            if marker in output and re.search(rf"niveau={level} .* sortie=0", output):
                return {"vcpus": cpus, "niveau": level, "sortie": 0, "restitution": True}
            if vm.poll() is not None or "Panique du noyau" in output:
                break
            time.sleep(.05)
        raise RuntimeError(f"exemple SDK incomplet\n{output[-1800:]}")
    return scenario_vm(arch, "sdk-example", check, f"-{cpus}cpu", NOVA_VCPUS=str(cpus))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vcpus", type=int, choices=(1, 2, 4),
                        help="exécuter uniquement cette géométrie (par défaut : 1, 2 et 4)")
    parser.add_argument("--exemple-sdk", action="store_true",
                        help="exécuter l'exemple de l'archive dans sa configuration ordinaire à 2 vCPU par défaut")
    args = parser.parse_args()
    arch = os.environ["NOVA_ARCH"]
    target = TARGETS[arch]
    out = BUILD / "rust-threads"
    out.mkdir(parents=True, exist_ok=True)
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
    env = {**os.environ, "RUSTC": rustc}
    with tempfile.TemporaryDirectory(prefix="novaos-native-") as tmp:
        elf, report = build_application(Path(tmp).resolve(), arch, env, args.exemple_sdk)
        cargo_target = out / ("cargo-cible" if args.vcpus else "cargo")
        if args.exemple_sdk:
            cargo_target = out / "cargo-exemple"
        subprocess.run([*CARGO, "build", "-p", "novaos", "--release", "--locked", "--offline",
                        "--target", target, "--features", "thread-probe"], cwd=ROOT, check=True,
                       env={**env, "CARGO_TARGET_DIR": str(cargo_target), "NOVA_THREAD_PROBE_ELF": str(elf)})
        kernel = cargo_target / target / "release/novaos"
        os.environ["NOVA_KERNEL_ELF"] = str(kernel)
        report["noyau_sha256"] = hashlib.sha256(kernel.read_bytes()).hexdigest()
        report["qemu"] = subprocess.check_output(
            [f"qemu-system-{arch}", "--version"], text=True).splitlines()[0]
        if args.exemple_sdk:
            cpus = args.vcpus or 2
            report["profils"] = [exercise_example(arch, cpus)]
            name = f"{arch}-exemple-{cpus}cpu"
        else:
            profiles = [args.vcpus] if args.vcpus else (1, 2, 4)
            report["profils"] = [exercise(arch, n, "-cible" if args.vcpus else "") for n in profiles]
            name = f"{arch}-{args.vcpus}cpu" if args.vcpus else arch
        (out / f"{name}.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
