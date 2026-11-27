"""Relecture des preuves 5B et contrôles du lecteur ACPI, sans appel Windows ou QEMU."""
from collections import Counter
from hashlib import sha256
import ctypes
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tomllib

from inventaire.tables import examiner, lire

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PROOFS = HERE.parent / "validation/killer-e2400-phase5b"
BASE = "0fb87dfa4936773f246a053c0705c7f62b36c126"


def require(value, cause):
    if not value:
        raise ValueError(cause)


def acpi():
    page = bytearray(48)
    page[:4] = b"DMAR"
    page[4:8] = len(page).to_bytes(4, "little")
    page[9] = -sum(page) % 256
    require(examiner("DMAR", page)["octets"] == 48, "DMAR nominal")
    def firmware(_, buf, n):
        if buf is not None:
            ctypes.memmove(buf, bytes(page), len(page))
        return len(page)
    require(lire(firmware, 0) == page, "Lecture en deux appels")
    for name, payload in [("APIC", page), ("DMAR", page[:-1]), ("DMAR", page[:-1] + b"X")]:
        try:
            examiner(name, payload)
        except ValueError:
            continue
        raise ValueError("Table incorrecte acceptée")
    for api in (lambda *args: 0, lambda *args: 2**24, lambda _, buf, n: 48 if buf is None else 32):
        try:
            lire(api, 0)
        except ValueError:
            continue
        raise ValueError("Lecture incomplète acceptée")


def main():
    subprocess.run(["git", "diff", "--exit-code", BASE, "--", "novaOS/kernel", "novaOS/abi",
                    "novaOS/user", "novaOS/libs", "novaOS/tests", "novaOS/Cargo.toml", "novaFS",
                    str(PROOFS.relative_to(ROOT))], cwd=ROOT, check=True)
    require(len(tomllib.loads((ROOT/"novaOS/abi/syscalls.toml").read_text())["call"]) == 48, "ABI")
    for line in (PROOFS/"SHA256SUMS").read_text().splitlines():
        digest, name = line.split()
        require(sha256((PROOFS/name).read_bytes()).hexdigest() == digest, name)
    with tarfile.open(PROOFS/"preuves-5b.tar.gz") as archive:
        def data(name):
            return archive.extractfile(name).read()
        for name, value in json.loads(data("empreintes.json")).items():
            require(sha256(data(name)).hexdigest() == value["distribue"], name)
        require(b"quarantaine=259,130,0 retour=OK dma_physique=0" in data("courtier/serie.log"), "Retrait")
        for isa in ("aarch64", "x86_64"):
            g = data("hotes/gestionnaire-"+isa+".log")
            e = data("hotes/e2400-"+isa+".log")
            for marker in (b"bornes_directions_et_restitution_par_page", b"preuve_differee_et_unique_credit_de_cloture"):
                require(marker+b" ... ok" in g, marker)
            require(b"titulaire_incarnation_generation_et_canal_lies ... ok" in e, isa)
        for case, held in (("duo-retrait", 0), ("duo-absent", 26)):
            prefix = "q35/"+case+"/"
            report = json.loads(data(prefix+"campagne.json"))
            ops = Counter(re.findall(rb"^E3D CQE op=(\w+)", data(prefix+"serie.log"), re.M))
            require(report["succes"] and ops == {b"READ":22,b"WRITE":7,b"FLUSH":3}, case)
            require(report["stockage"]["cadres_nic_retenus"] == held and
                    sha256(data(prefix+"namespace.raw")).hexdigest() == report["p9_final"], case)
        for n in (1, 2, 3):
            arm = json.loads(data(f"arm/{n}/campagne.json"))
            require(arm["succes"] and arm["trames_par_sens"] == 13312, "VirtIO ARM")
    acpi()
    print("OK — ABI 48, preuves 5B, droits et baux, quarantaine 259/130/0, VirtIO ARM, NVMe Q35, tables ACPI fictives")


if __name__ == "__main__":
    main()
