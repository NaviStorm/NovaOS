"""Compile le témoin natif hors du monorepo, l'admet, puis l'exécute en TCG."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile
import time

from demarrage import ROOT, BUILD, scenario_vm


CARGO = ["rustup", "run", "stable", "cargo"]
TARGETS = {"x86_64": "x86_64-unknown-none", "aarch64": "aarch64-unknown-none-softfloat"}


def build_application(work, arch, env):
    if work.is_relative_to(ROOT.parent):
        raise ValueError("la construction du témoin exige un workspace externe")
    project = work / "temoin"
    shutil.copytree(ROOT / "tests/external-threads", project / "src")
    library = json.dumps(str(ROOT / "libs/nova-system"), ensure_ascii=False)
    (project / "Cargo.toml").write_text(
        '[workspace]\n[package]\nname = "temoin"\nversion = "0.1.0"\nedition = "2024"\n'
        f'[dependencies]\nnova-system = {{ path = {library} }}\n'
        '[profile.release]\npanic = "abort"\n'
    )
    target = TARGETS[arch]
    linker = ROOT / "user/novafs/service.ld"
    args = [*CARGO, "build", "--release", "--offline", "--target", target]
    subprocess.run(args, cwd=project, check=True, env={
        **env, "CARGO_TARGET_DIR": str(work / "application-target"),
        "CARGO_ENCODED_RUSTFLAGS": "\x1f".join([
            "-Crelocation-model=static", "-Cno-redzone=yes",
            "-Clink-arg=-zmax-page-size=4096", f"-Clink-arg=-T{linker}",
        ]),
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
                 "segments_elf": types}


def exercise(arch, cpus):
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
                   "THREAD garde adresse=", "faute=confirmee",
                   "THREAD destruction attentes=2 ipc=1 calcul=1 survivants=0"]
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
    return scenario_vm(arch, "threads-test", check, f"-{cpus}cpu", NOVA_VCPUS=str(cpus))


def main():
    arch = os.environ["NOVA_ARCH"]
    target = TARGETS[arch]
    out = BUILD / "rust-threads"
    out.mkdir(parents=True, exist_ok=True)
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
    env = {**os.environ, "RUSTC": rustc}
    with tempfile.TemporaryDirectory(prefix="novaos-native-") as tmp:
        elf, report = build_application(Path(tmp).resolve(), arch, env)
        cargo_target = out / "cargo"
        subprocess.run([*CARGO, "build", "-p", "novaos", "--release", "--locked", "--offline",
                        "--target", target, "--features", "thread-probe"], cwd=ROOT, check=True,
                       env={**env, "CARGO_TARGET_DIR": str(cargo_target), "NOVA_THREAD_PROBE_ELF": str(elf)})
        kernel = cargo_target / target / "release/novaos"
        os.environ["NOVA_KERNEL_ELF"] = str(kernel)
        report["noyau_sha256"] = hashlib.sha256(kernel.read_bytes()).hexdigest()
        report["qemu"] = subprocess.check_output(
            [f"qemu-system-{arch}", "--version"], text=True).splitlines()[0]
        report["profils"] = [exercise(arch, n) for n in (1, 2, 4)]
        (out / f"{arch}.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
