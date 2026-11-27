"""Construit et signe uniquement l'ELF de développement x86_64, hors catalogue actif."""
import argparse
import hashlib
import struct
import subprocess
import sys
from pathlib import Path
import tomllib

HERE = Path(__file__).resolve().parent
NOVA = HERE.parents[1]
DRIVER = NOVA / "user/killer-e2400"
sys.path.insert(0, str(NOVA / "user/novaauth/service"))
sys.path.insert(0, str(NOVA / "user/gestionnaire-4d"))
from paquet_lab import build as entete_pkg01
from preparer import signature  # Graines PUBLIQUES de développement des trois rôles 4B.

def construire(out):
    out.mkdir(parents=True, exist_ok=True)
    rust = ["rustup", "run", "stable", "rustc", "--edition=2024", "--target=x86_64-unknown-none",
            "-Cpanic=abort", "-Copt-level=s", "-Coverflow-checks=yes", "-Crelocation-model=static",
            "-Ccodegen-units=1", "-Cno-redzone=yes", "--remap-path-prefix", f"{NOVA}=novaOS"]
    lib = out / "libethernet_v1.rlib"
    subprocess.run([*rust, "--crate-name=ethernet_v1", "--crate-type=rlib",
                    NOVA / "tests/ethernet_v1/lib.rs", "-o", lib], check=True)
    elf = out / "killer-e2400.elf"
    subprocess.run([*rust, "--crate-name=killer_e2400", "--extern", f"ethernet_v1={lib}",
                    "-Cstrip=symbols", "-Clink-arg=-zmax-page-size=4096",
                    f"-Clink-arg=-T{NOVA}/user/gestionnaire-4d/acteur.ld",
                    DRIVER / "acteur.rs", "-o", elf], check=True)
    return elf

def signer(elf, dest):
    template = (DRIVER / "pilote.toml.in").read_text()
    config = tomllib.loads(template)
    membre = config["image"][0]
    header, payload = entete_pkg01(bytes.fromhex(config["elf_package_id"]),
                                   config["elf_package_revision"], membre["isa"], [(membre["member"], elf)])
    pkg = struct.pack("<I", len(header)) + header + signature(2, b"", bytes(header)) + payload
    profil = (DRIVER / "profil-cpu.toml").read_bytes()
    h = lambda data: hashlib.sha256(data).hexdigest()
    dependency = config["dependency"][0]
    assert tomllib.loads(profil.decode())["id"] == dependency["id"]
    recu = (f'origin="laboratoire"\npolicy_revision=1\nsecurity_epoch=3\nid="{dependency["id"]}"\n'
            f'kind="system-profile"\nversion=[1,0,0]\nsha256="{h(profil)}"\ndependencies=[]\n').encode()
    manifeste = template.replace("@PAQUET@", h(pkg)).replace("@ELF@", h(payload)).replace("@PROFIL@", h(profil)).encode()
    dest.mkdir(exist_ok=True, parents=True)
    for nom, donnees in {
        "pilote.pkg01": pkg, "pilote.toml": manifeste, "profil.toml": recu,
        "pilote.sig": signature(1, b"NovaOS/driver-package/v1", manifeste),
        "profil.sig": signature(3, b"NovaOS/driver-resource/v1", recu),
    }.items():
        (dest / nom).write_bytes(donnees)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("travail", type=Path)
    parser.add_argument("paquet", type=Path)
    args = parser.parse_args()
    signer(construire(args.travail.resolve()), args.paquet.resolve())
