"""Exécute le registre et le courtier simulés dans un ELF Ring 3, sans NIC ni disque-cible."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
NOVA = HERE.parents[1]
sys.path[:0] = [str(NOVA / "tests"), str(NOVA / "tests/ethernet_v1")]
import demarrage
from port_qemu import qualification


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sortie", type=Path)
    parser.add_argument("worktrees", type=Path)
    args = parser.parse_args()
    dest = args.sortie.resolve()
    dest.mkdir(parents=True, exist_ok=False)
    triple = "x86_64-unknown-none"
    cargo = ["rustup", "run", "stable", "cargo", "build", "--locked", "--offline", "--release", "--target", triple]
    flags = ["--cfg", "novaos_mesure_tas_4d", "-Crelocation-model=static", "-Cno-redzone=yes", "-Clink-arg=-zmax-page-size=4096",
             "-Clink-arg=-T" + str(NOVA/"user/gestionnaire-4d/acteur.ld"), "--cfg", 'sha2_backend="soft"',
             "--cfg", 'curve25519_dalek_backend="serial"', "--cfg", 'chacha20_backend="soft"', "--cfg", 'poly1305_backend="soft"']
    env = {**os.environ, "RUSTC": subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip()}
    subprocess.run([*cargo,"--manifest-path",str(HERE/"Cargo.toml"),"--features","invite-5b","--bin","courtier-5b"],
                   cwd=NOVA.parent, check=True, env={**env, "CARGO_TARGET_DIR":str(dest/"acteur"),
                   "CARGO_ENCODED_RUSTFLAGS":"\x1f".join(flags),"CARGO_PROFILE_RELEASE_PANIC":"abort",
                   "CARGO_PROFILE_RELEASE_OPT_LEVEL":"s","CARGO_PROFILE_RELEASE_LTO":"true",
                   "CARGO_PROFILE_RELEASE_CODEGEN_UNITS":"1","CARGO_PROFILE_RELEASE_STRIP":"symbols"})
    elf = dest/"acteur"/triple/"release/courtier-5b"
    subprocess.run([*cargo,"-p","novaos","--features","thread-probe"],cwd=NOVA,check=True,
                   env={**env,"NOVA_THREAD_PROBE_ELF":str(elf),"CARGO_TARGET_DIR":str(dest/"noyau")})
    kernel = dest/"noyau"/triple/"release/novaos"
    code = Path(os.environ.get("OVMF_CODE") or (Path(subprocess.check_output(["brew","--prefix","qemu"],text=True).strip())/"share/qemu/edk2-x86_64-code.fd"))
    nvram = Path(os.environ.get("OVMF_VARS") or code.with_name("edk2-i386-vars.fd"))
    os.environ.update(NOVA_KERNEL_ELF=str(kernel), NOVA_ARCH="x86_64",NOVA_X86_CPU="max",NOVAOS_TMPDIR=str(dest),
                      OVMF_CODE=str(code),OVMF_VARS=str(nvram))
    demarrage.BUILD = dest/"demarrage"
    meta = {"revision":subprocess.check_output(["git","rev-parse","HEAD"],cwd=NOVA.parent,text=True).strip(),
            "qemu":subprocess.check_output(["qemu-system-x86_64","--version"],text=True).splitlines()[0],
            "modele":"Q35 TCG, 1 vCPU max, 256 Mio, -nic none","elf_sha256":digest(elf),
            "elf_octets":elf.stat().st_size, "kernel_sha256":digest(kernel),"firmware":{p.name:digest(p) for p in (code,nvram)}}
    def observe(vm, directory, log):
        until = time.monotonic()+60
        while time.monotonic()<until and vm.poll() is None:
            data=log.read_text(errors="replace") if log.exists() else ""
            if "SDK exemple admis sortie=0 restitution=OK" in data:
                if not all(s in data for s in ("niveau=Ring3","E2400 5B registre=259 tx=264 rx=264 quarantaine=259,130,0 retour=OK dma_physique=0")):
                    raise ValueError("Témoin 5B incomplet")
                result = re.findall(r"E2400 RAM tas_pic=(\d+) tas_vivant_fin=(\d+) pages_pic=(\d+) pages_fin=(\d+)", data)
                if len(result) != 1:
                    raise ValueError("Mesure du tas Ring 3 absente")
                peak,live,pages,end = map(int,result[0])
                if live != 0 or not 13025 <= peak <= pages*4096 or end > pages:
                    raise ValueError("Comptage mémoire Ring 3 incohérent")
                meta["tas_ring3"] = dict(pic_octets=peak, vivants_fin=live, pic_pages=pages, pages_fin=end)
                return
            if "Panique du noyau" in data or "Erreur fatale" in data:
                raise ValueError(data[-1600:])
            time.sleep(.05)
        raise RuntimeError("Délai du témoin Ring 3")
    try:
        with qualification(args.worktrees.resolve(),meta):
            demarrage.scenario_vm("x86_64","sdk-example",observe,"-killer5b",NOVA_VCPUS="1",NOVA_RAM="256")
    finally:
        log=demarrage.BUILD/"serie-x86_64-sdk-example-killer5b.log"
        if log.exists():
            (dest/"serie.log").write_bytes(log.read_bytes())
            meta["journal"]=digest(log)
        (dest/"banc.json").write_text(json.dumps(meta,indent=2)+"\n")


if __name__=="__main__":
    main()
