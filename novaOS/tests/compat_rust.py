"""Croise deux binaires clients Rust avec les noyaux et services v0.19 et v0.20 dans QEMU."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from cli import ROOT, Terminal
import session_cli


PRE_THREADS = "2bcb64575e396790089e3078d443b63d69ac05d7"
PHASE1 = "90de918fa6ca090d731500127cf400c4836a0fec"
TARGETS = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}
CARGO = ["rustup", "run", "stable", "cargo"]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_native_abi():
    # Les deux versions du fichier v1 doivent être identiques une fois les
    # neuf déclarations additives de la phase 1 retirées.
    path = "novaOS/abi/native.rs"
    old = subprocess.check_output(["git", "show", PRE_THREADS + ":" + path], cwd=ROOT.parent)
    current = (ROOT.parent / path).read_bytes()
    extra = re.search(rb"/// Extension native v1 Rust/threads\.[^\n]*\n"
                      rb"((?:pub const THREAD_[A-Z]+: u64 = 0x[0-9a-f]+;\n){9})\n", current)
    assert extra and current[:extra.start()] + current[extra.end():] == old, "ABI historique modifiée"
    assert [int(n, 16) for n in re.findall(rb"0x([0-9a-f]+)", extra[1])] == list(range(0x10060, 0x10069))


def sources(revision, destination):
    destination.mkdir()
    paths = ["components", "novaFS", "novaOS/Cargo.toml", "novaOS/Cargo.lock", "novaOS/.cargo",
             "novaOS/abi", "novaOS/kernel", "novaOS/user"]
    if revision == PHASE1:
        paths.append("novaOS/libs")
    with subprocess.Popen(["git", "archive", revision, *paths], cwd=ROOT.parent,
                          stdout=subprocess.PIPE) as archive:
        subprocess.run(["tar", "-xf", "-", "-C", destination], stdin=archive.stdout, check=True)
        if archive.wait():
            raise RuntimeError(f"export incomplet de {revision}")


def client(directory, sdk, target, phase2, env):
    src = directory / "src"
    src.mkdir(parents=True)
    shutil.copyfile(ROOT / "tests/compat-rust-0200/client.rs", src / "main.rs")
    (directory / "Cargo.toml").write_text(
        '[workspace]\n[package]\nname = "compat-client"\nversion = "0.1.0"\nedition = "2024"\n'
        '[features]\nphase2 = []\n[profile.release]\npanic = "abort"\n'
        f'[dependencies]\nnova-system = {{ path = {json.dumps(str(sdk))} }}\n')
    flags = ["-Crelocation-model=static", "-Cno-redzone=yes", "-Clink-arg=-zmax-page-size=4096",
             f"-Clink-arg=-T{ROOT / 'user/novafs/service.ld'}"]
    cmd = [*CARGO, "build", "--offline", "--release", "--target", target]
    if phase2:
        cmd.extend(["--features", "phase2"])
    subprocess.run(cmd, cwd=directory, check=True,
                   env={**env, "CARGO_TARGET_DIR": str(directory / "target"),
                        "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(flags)})
    return directory / "target" / target / "release/compat-client"


def kernel(tree, clients, target, env):
    # L'assemblage privé remplace deux entrées connues après construction des
    # ELF ordinaires. Le lanceur, leurs droits et le service restent historiques.
    build = tree / "novaOS/kernel/build.rs"
    original = build.read_text()
    anchor = '        if name != "nsh" {'
    assert original.count(anchor) == 1
    additions = "".join(
        f'        if name == "{slot}" {{ std::fs::copy({json.dumps(str(elf))}, &path).unwrap(); }}\n'
        for slot, elf in zip(("head", "tail"), clients, strict=True)
    )
    build.write_text(original.replace(anchor, additions + anchor))
    target_dir = tree / "target"
    subprocess.run([*CARGO, "build", "--locked", "--offline", "--release", "-p", "novaos",
                    "--target", target], cwd=tree / "novaOS", check=True,
                   env={**env, "CARGO_TARGET_DIR": str(target_dir)})
    release = target_dir / target / "release"
    media, = (release / "build").glob("novaos-*/out/storage-media.bin")
    for slot, elf in zip(("head", "tail"), clients, strict=True):
        assert digest(elf) == digest(media.with_name(slot + ".elf"))
    return release / "novaos", media


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=TARGETS)
    parser.add_argument("--host", type=Path, default=session_cli.HOST,
                        help="outil Darwin novafs construit à partir du moteur P9 de référence")
    args = parser.parse_args()
    session_cli.HOST = args.host.resolve(strict=True)
    check_native_abi()
    # Les seules modifications noyau admises depuis la phase 1 concernent des
    # contrôleurs de qualification conditionnels ; le service et l'ABI sont identiques.
    subprocess.run(["git", "diff", "--exit-code", PHASE1, "--", "novaOS/user/novafs",
                    "novaOS/abi", "novaFS", "novaOS/kernel/src/sched/process/threads.rs",
                    "novaOS/kernel/src/sched/process/syscall.rs"], cwd=ROOT.parent, check=True)
    env = {**os.environ, "CARGO_NET_OFFLINE": "true",
           "RUSTC": subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"],
                                            text=True).strip()}
    os.environ.update(NOVA_VCPUS="1", NOVA_RAM="256", NOVA_ARCH=args.arch)
    target = TARGETS[args.arch]
    out = ROOT / "build" / "matrice-rust"
    out.mkdir(parents=True, exist_ok=True)
    report = {"arch": args.arch, "qemu": subprocess.check_output(
        [f"qemu-system-{args.arch}", "--version"], text=True).splitlines()[0], "invites": {}}
    with tempfile.TemporaryDirectory(prefix=args.arch + "-", dir=out) as temp:
        temp = Path(temp)
        trees = {"v019": temp / "v019", "phase1": temp / "phase1"}
        sources(PRE_THREADS, trees["v019"])
        sources(PHASE1, trees["phase1"])
        clients = [client(temp / "client-1", trees["phase1"] / "novaOS/libs/nova-system", target,
                          False, env),
                   client(temp / "client-2", ROOT / "libs/nova-system", target, True, env)]
        report["clients_sha256"] = [digest(elf) for elf in clients]
        fixture = temp / "fixture.raw"
        session_cli.fixture_p9(fixture, 512)
        for label, tree in trees.items():
            elf, service = kernel(tree, clients, target, env)
            os.environ["NOVA_KERNEL_ELF"] = str(elf)
            image = tree / "p9.raw"
            shutil.copyfile(fixture, image)
            journal = tree / "session.serial"
            with Terminal(args.arch, 512, image, "sdk-" + label, test=True, journal=journal) as term:
                term.until(b"nsh> ")
                term.line("cat documents/exemple.txt", status="sortie 0", has=[b"\r\nNovaOS\r\n"])
                expected = 19 if label == "v019" else 0
                for command in ("head", "tail"):
                    term.line(command + " documents/exemple.txt", status=f"sortie {expected}")
                term.exit()
            report["invites"][label] = {"reference": PRE_THREADS if label == "v019" else PHASE1,
                                        "kernel_sha256": digest(elf), "service_sha256": digest(service),
                                        "journal_sha256": digest(journal), "statut_des_clients": expected}
    (out / (args.arch + ".json")).write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
