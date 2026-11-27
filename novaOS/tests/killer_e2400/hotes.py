"""Qualification hors ligne du paquet, de l'adaptateur et du catalogue sur les deux ISA Darwin."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import tomllib
from construire import construire, signer

HERE = Path(__file__).resolve().parent
NOVA = HERE.parents[1]
BASE = "908976948ba58fc85e7fe38bfc315f689ed1c44e"

def sortie(*args):
    return subprocess.check_output(args, text=True, cwd=NOVA.parent).strip()

def empreinte(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def segments(elf):
    data = elf.read_bytes()
    debut, = struct.unpack_from("<Q", data, 32)
    pas, n = struct.unpack_from("<HH", data, 54)
    valeurs = []
    for i in range(n):
        typ, droits, _, va, _, fichier, mem, align = struct.unpack_from("<IIQQQQQQ", data, debut + pas*i)
        valeurs.append({"type": typ, "droits": droits, "va": va, "fichier": fichier, "memoire": mem, "align": align})
    if not all(s["droits"] in (4,5,6) and s["fichier"] <= s["memoire"] for s in valeurs):
        raise ValueError("Segment ELF inattendu")
    return valeurs

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie", type=Path)
    a = p.parse_args()
    out = a.sortie.resolve()
    out.mkdir(parents=True, exist_ok=False)
    info = {"base":BASE, "revision":sortie("git","rev-parse","HEAD"),
            "rustc":sortie("rustup","run","stable","rustc","-Vv"), "macos":sortie("sw_vers","-productVersion")}
    pkg = out/"paquet"
    elf = construire(out/"objets")
    signer(elf, pkg)
    for original in sorted((HERE/"paquet").iterdir()):
        if (pkg/original.name).read_bytes() != original.read_bytes():
            raise ValueError(f"Écart de reconstruction sur {original.name}")
    info["paquet"] = {f.name:empreinte(f) for f in sorted(pkg.iterdir())}
    info["elf"] = {"sha256":empreinte(elf),"octets":elf.stat().st_size,"segments":segments(elf)}
    info["abi"] = len(tomllib.loads((NOVA/"abi/syscalls.toml").read_text())["call"])
    if info["abi"] != 48:
        raise ValueError("Nombre de sélecteurs")
    subprocess.run(["git","diff","--exit-code",BASE,"--","novaOS/abi","novaOS/kernel","novaOS/Cargo.toml","novaOS/libs",
                   "novaOS/user/ethernet","novaOS/user/virtio_net","novaOS/user/gestionnaire-4d","novaOS/user/novaauth","novaFS"],
                   cwd=NOVA.parent,check=True)
    env = {**os.environ, "RUSTC":sortie("rustup","which","--toolchain","stable","rustc"),
           "RUSTDOC":sortie("rustup","which","--toolchain","stable","rustdoc"),"CARGO_TARGET_DIR":str(out/"cibles")}
    info["hotes"] = {}
    for isa, arch in [("aarch64","arm64"),("x86_64","x86_64")]:
        machine = sortie("arch","-"+arch,"uname","-m")
        translation = sortie("arch","-"+arch,"/usr/sbin/sysctl","-in","sysctl.proc_translated")
        if machine != arch or (isa=="x86_64" and translation!="1") or (isa=="aarch64" and translation not in ("","0")):
            raise ValueError((machine,translation))
        log = out/f"{isa}.log"
        cmd = ["rustup","run","stable","cargo","test","--locked","--offline","--release",
               "--target",isa+"-apple-darwin","--manifest-path",str(HERE/"Cargo.toml")]
        with log.open("w") as f:
            subprocess.run(cmd, check=True, env=env, cwd=NOVA.parent, stdout=f,stderr=subprocess.STDOUT)
        suites = re.findall(r"test result: ok\. (\d+) passed; 0 failed",log.read_text())
        if len(suites) != 4 or sum(int(n) for n in suites[:3])==0 or suites[-1]!="0":
            raise ValueError(f"Rapport incomplet {log}")
        info["hotes"][isa] = {"machine":machine,"translation":translation,"tests":[int(n) for n in suites],
                              "journal":empreinte(log)}
    (out/"bilan.json").write_text(json.dumps(info,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps(info["hotes"],ensure_ascii=False))

if __name__ == "__main__":
    main()
