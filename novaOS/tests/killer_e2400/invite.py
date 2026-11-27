"""Exerce l'ELF du paquet sous Q35 sans NIC, dans une extraction privée de la base 4F."""
import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time

HERE = Path(__file__).resolve().parent
NOVA = HERE.parents[1]
BASE = "908976948ba58fc85e7fe38bfc315f689ed1c44e"
sys.path[:0] = [str(NOVA / "tests"), str(NOVA / "tests/ethernet_v1")]
import demarrage
from port_qemu import qualification

def sha(path):
    return sha256(path.read_bytes()).hexdigest()

def image_signee(dest):
    # Le banc lance les octets extraits du plan accepté par le catalogue Rust.
    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
    host = next(line.split()[1] for line in subprocess.check_output([rustc, "-vV"], text=True).splitlines()
                if line.startswith("host:"))
    cmd = ["rustup", "run", "stable", "cargo", "run", "--locked", "--offline", "--release",
           "--target", host, "--manifest-path", str(HERE / "Cargo.toml"), "--example", "extraire", "--", str(dest)]
    subprocess.run(cmd, check=True, cwd=NOVA.parent,
                   env={**os.environ, "RUSTC": rustc, "CARGO_TARGET_DIR": str(dest.parent / "selection")})


def construire(root, elf, target):
    with subprocess.Popen(["git","archive",BASE,"novaOS","novaFS","components"], cwd=NOVA.parent,
                          stdout=subprocess.PIPE) as proc:
        with tarfile.open(fileobj=proc.stdout, mode="r|") as archive:
            archive.extractall(root, filter="data")
        if proc.wait():
            raise ValueError("Extraction Git incomplète")
    sonde = root/"novaOS/kernel/src/sched/process/ipc/thread_probe.rs"
    old = "pub(crate) fn run_example() {"
    original = sonde.read_text()
    if original.count(old) != 1:
        raise ValueError("Sonde de référence divergente")
    sonde.write_text(original.replace(old, "fn exemple_sdk_historique() {")
                     + '\ninclude!(env!("NOVA_E2400_BANC"));\n')
    env = {**os.environ, "NOVA_THREAD_PROBE_ELF": str(elf), "NOVA_E2400_BANC": str(HERE/"invite.rs"),
           "CARGO_TARGET_DIR": str(target), "RUSTC": subprocess.check_output(
               ["rustup","which","--toolchain","stable","rustc"],text=True).strip()}
    subprocess.run(["rustup","run","stable","cargo","build","--locked","--offline","--release",
                    "-p","novaos","--target","x86_64-unknown-none","--features",
                    "thread-probe,ethernet-v1-port,net-q35-lab"], cwd=root/"novaOS", env=env, check=True)
    return target/"x86_64-unknown-none/release/novaos"

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie", type=Path)
    p.add_argument("worktrees", type=Path, help="répertoire du verrou QEMU commun")
    a = p.parse_args()
    out = a.sortie.resolve()
    out.mkdir(parents=True, exist_ok=False)
    elf = out/"killer-e2400.elf"
    image_signee(elf)
    with tempfile.TemporaryDirectory(dir=out, prefix="src-4f-") as td:
        kernel = construire(Path(td), elf, out/"cible")
    os.environ.update(NOVA_KERNEL_ELF=str(kernel), NOVA_ARCH="x86_64", NOVA_X86_CPU="max", NOVAOS_TMPDIR=str(out))
    code = Path(os.environ.get("OVMF_CODE") or (Path(subprocess.check_output(["brew","--prefix","qemu"],text=True).strip())/"share/qemu/edk2-x86_64-code.fd"))
    nvram = Path(os.environ.get("OVMF_VARS") or code.with_name("edk2-i386-vars.fd"))
    os.environ.update(OVMF_CODE=str(code), OVMF_VARS=str(nvram))
    rapport = {"base":BASE, "revision":subprocess.check_output(["git","rev-parse","HEAD"],cwd=NOVA.parent,text=True).strip(),
               "ovmf":{f.name:sha(f) for f in [code,nvram]}, "cpu":"max", "modele":"q35", "vcpus":1, "ram_mio":256,
               "acceleration":"tcg", "nic":"none", "elf":sha(elf), "noyau":sha(kernel), "banc":sha(HERE/"invite.rs"),
               "qemu":subprocess.check_output(["qemu-system-x86_64","--version"],text=True).splitlines()[0]}
    def observer(vm, directory, log):
        limite = time.monotonic()+180
        while time.monotonic()<limite and vm.poll() is None:
            s = log.read_text(errors="replace") if log.exists() else ""
            if "Panique du noyau" in s or "Erreur fatale" in s:
                raise RuntimeError(s[-2000:])
            if "E2400 5A Q35 CPU termine" in s:
                for attendu in ("niveau=Ring3", "E2400 5A simulation CPU 264", "boite=true trames=288",
                                "boite=false trames=288", "perte boite=true prefixe=4", "perte boite=false prefixe=4"):
                    if attendu not in s:
                        raise ValueError(attendu)
                return
            time.sleep(.05)
        raise RuntimeError("Fin du scénario absente")
    demarrage.BUILD = out/"demarrage"
    demarrage.BUILD.mkdir()
    nom = "x86_64-sdk-example-killer5a"
    try:
        with qualification(a.worktrees.resolve(), rapport):
            demarrage.scenario_vm("x86_64","sdk-example",observer,"-killer5a",NOVA_VCPUS="1",NOVA_RAM="256")
    finally:
        log = demarrage.BUILD/f"serie-{nom}.log"
        if log.exists():
            shutil.copyfile(log,out/"serie.log")
            rapport["journal"] = sha(log)
        (out/"banc.json").write_text(json.dumps(rapport,indent=2)+"\n")

if __name__ == "__main__":
    main()
