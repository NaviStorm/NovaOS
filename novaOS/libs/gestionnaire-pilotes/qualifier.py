"""Qualification Darwin (ARM64/Rosetta) et compilation des deux objets nus, hors ligne."""
import hashlib
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parent
CAT = ROOT.parent / "catalogue-pilotes"
OS = ROOT.parents[1]
REPO = OS.parent
BASE = "e9af41f00949e6daade0dd7cb267bcf3262c36e5"
BUILD = Path(os.environ["CARGO_TARGET_DIR"]).resolve()
TMP = Path(os.environ["NOVAOS_TMPDIR"]).resolve()
if TMP not in BUILD.parents:
    raise SystemExit("Choisir un répertoire de compilation privé sous NOVAOS_TMPDIR.")
if platform.system() != "Darwin" or platform.machine() != "arm64":
    raise SystemExit("Le protocole demande Darwin ARM64 natif avec Rosetta.")

def cmd(args, *, env=None):
    result = subprocess.run(args, cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE, check=True)
    return result.stdout.strip()

def log(*values):
    print(*values, flush=True)

env = dict(os.environ, CARGO_PROFILE_RELEASE_STRIP="none")
for variable, tool in (("RUSTC", "rustc"), ("RUSTDOC", "rustdoc")):
    env[variable] = cmd(["rustup", "which", "--toolchain", "stable", tool])
cargo = ["rustup", "run", "stable", "cargo"]
size = shutil.which("llvm-size") or "/opt/homebrew/opt/llvm/bin/llvm-size"

def build(component, verb, target, *args, flags=None):
    build_env = env.copy()
    if flags is not None:
        build_env["RUSTFLAGS"] = flags
    return cmd([*cargo, verb, "--manifest-path", component / "Cargo.toml", "--locked",
                "--offline", "--release", "--target", target, *args], env=build_env)

def suite(component, target, expected):
    result = build(component, "test", target, "--", "--test-threads=1")
    lines = re.findall(r"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored", result)
    total = sum(int(n) for n, _ in lines)
    if total != expected or not lines:
        raise RuntimeError(f"Nombre de tests inattendu pour {component.name}: {total}")
    log(component.name, target, "tests réussis", total, "ignorés", sum(int(n) for _, n in lines))
    for line in result.splitlines():
        if line.startswith("test ") and line.endswith(" ... ok"):
            log(" ", line)

def measures(target, case):
    out = build(ROOT, "test", target, "--test", "memoire", case, "--",
                "--exact", "--nocapture", "--test-threads=1")
    if "1 passed; 0 failed" not in out:
        raise RuntimeError(case)
    for line in out.splitlines():
        line = line.removeprefix("test " + case + " ... ")
        if line and not line.startswith(("running ", "test result:")) and line != "ok":
            log(line)

def object_size(component, target):
    obj = BUILD / target / (component.name + "-4c.o")
    flags = ('-C relocation-model=static --cfg sha2_backend="soft" --cfg sha2_force_soft '
             '--cfg chacha20_backend="soft" --cfg poly1305_backend="soft" '
             '--cfg curve25519_dalek_backend="serial"')
    if target == "x86_64-unknown-none":
        flags += " -C no-redzone=yes"
    build(component, "rustc", target, "--lib", "--", "--emit=obj=" + str(obj), flags=flags)
    totals = dict(text=0, rodata=0, data=0, bss=0)
    for line in cmd([size, "-A", obj]).splitlines():
        cols = line.split()
        if len(cols) >= 2 and cols[1].isdigit():
            prefix = cols[0].split(".")[1:2]
            if prefix and prefix[0] in totals:
                totals[prefix[0]] += int(cols[1])
    if not totals["text"]:
        raise RuntimeError("Objet vide")
    log("objet", component.name, target, totals)

def invariants():
    # Les modifications des autres chantiers ne sont pas nécessaires au protocole.
    protected = ["novaOS/abi", "novaOS/kernel", "novaOS/user", "novaFS",
                 "novaOS/libs/catalogue-pilotes", "novaOS/Cargo.toml"]
    cmd(["git", "-C", REPO, "diff", "--exit-code", "--quiet", BASE, "--", *protected])
    selectors = tomllib.loads((OS / "abi/syscalls.toml").read_text())["call"]
    if len(selectors) != 48:
        raise RuntimeError("Registre ABI inattendu")
    if not re.search(r"pub const CREDITS: usize = 8;", (OS / "user/ethernet/backend.rs").read_text()):
        raise RuntimeError("Crédits Ethernet inattendus")
    log("Sources historiques vérifiées contre", BASE, "sélecteurs", len(selectors), "crédits Ethernet", 8)
    for name in ("reserver.data", "confirme.data"):
        raw = (ROOT / "vecteurs" / name).read_bytes()
        log("vecteur", name, len(raw), hashlib.sha256(raw).hexdigest())

def main():
    log("Révision", cmd(["git", "rev-parse", "HEAD"]))
    log("Hôte", platform.platform(), cmd([env["RUSTC"], "-V"]))
    translated = cmd(["arch", "-x86_64", "/usr/sbin/sysctl", "-n", "sysctl.proc_translated"])
    if translated != "1":
        raise RuntimeError("Rosetta attendue")
    log("x86_64 sysctl.proc_translated", translated)
    invariants()
    cmd([*cargo, "fmt", "-p", "novaos-gestionnaire-sim", "--", "--check"], env=env)
    for target in ("aarch64-apple-darwin", "x86_64-apple-darwin"):
        suite(CAT, target, 26)
        suite(ROOT, target, 28)
        for series in range(1, 4):
            log("Série", series, target, "quantiles ns [p50, p95, p99, max]")
            measures(target, "endurance_256_et_mesures")
            measures(target, "selection_un_et_soixante_quatre_paquets")
        measures(target, "echec_de_chaque_allocation_et_conservation")
    for target in ("aarch64-unknown-none-softfloat", "x86_64-unknown-none"):
        for component in (ROOT, CAT):
            object_size(component, target)

if __name__ == "__main__":
    main()
