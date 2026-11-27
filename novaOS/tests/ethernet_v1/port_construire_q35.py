"""Construit le profil stockage Q35 en associant le port aux modules audités à l'octet près."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile

NOVA = Path(__file__).resolve().parents[2]
AUDIT = "56db7d5c0d04490cd284b91567cbbd4736534266"
# Les dépendances sont des unités Rust du superviseur et de son appelant.
# L'historique de leurs essais, les campagnes et les modèles de faute ne sont pas copiés.
MODULES = (
    "hardware/acpi.rs",
    "hardware/acpi/q35_dmar.rs",
    "hardware/pci_inventory/lab_resources.rs",
    "hardware/lab_irq.rs",
    "hardware/vtd_lab.rs",
    "hardware/vtd_lab/format.rs",
    "hardware/vtd_lab/duo.rs",
    "hardware/nvme_domain.rs",
    "hardware/nvme_domain/traffic.rs",
    "hardware/nvme_domain/traffic/requests.rs",
    "sched/process/nvme.rs",
    "sched/process/nvme/broker.rs",
    "sched/process/nvme/profile.rs",
    "hardware/block/net_event.rs",
    "memory/mod.rs",
)


def git(*args):
    return subprocess.check_output(["git", "-C", NOVA.parent, *args])


def appliquer_raccord(root, patch, modules):
    blobs = re.findall(r"^diff --git a/(\S+) b/\1\nindex ([a-f0-9]{40})\.\.([a-f0-9]{40}) ", patch.read_text(), re.M)
    if {nom for nom, _, _ in blobs} != {"novaOS/kernel/src/"+nom for nom in modules}:
        raise ValueError(f"Périmètre du raccord {patch.name}")
    for nom, avant, _ in blobs:
        if git("hash-object", root/nom).decode().strip() != avant:
            raise ValueError(f"Préimage différente pour {nom}")
    subprocess.run(["git", "apply", "--check", "--unidiff-zero", str(patch)], cwd=root, check=True)
    subprocess.run(["git", "apply", "--unidiff-zero", str(patch)], cwd=root, check=True)
    for nom, _, apres in blobs:
        if git("hash-object", root/nom).decode().strip() != apres:
            raise ValueError(f"Raccord divergent pour {nom}")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie", type=lambda v: Path(v).resolve())
    p.add_argument("--revision", default="HEAD")
    p.add_argument("--phase3d", action="store_true", help="Appliquer le raccord de retrait individuel après vérification des modules audités")
    p.add_argument("--cas", nargs="+", choices=("port", "deces", "duo-retrait", "duo-differe", "duo-absent",
                   "duo-sans-reset", "duo-cycles", "duo-reouverture", "duo-postdisque", "duo-concurrence", "duo-disque", "duo-wait-absent", "duo-wait-tardif"))
    p.add_argument("--sonder-garde", action="store_true", help="Observer le refus de réadmission après restitution sur un bail NVMe actif")
    p.add_argument("--phase4f", choices=("nominal", "deces-observe", "lanceur-observe", "reconciliation", "gestionnaire-actif", "lanceur-actif"),
                   help="Assembler le gestionnaire dynamique Q35 et le retrait individuel 3D")
    a = p.parse_args()
    if a.phase4f and (not a.phase3d or a.sonder_garde or not a.cas or
                     set(a.cas) - {"port", "duo-cycles", "duo-retrait", "duo-differe", "duo-absent"}):
        p.error("4F exige le retrait individuel, différé ou absent, sur Q35 double")
    if a.sonder_garde and (not a.phase3d or a.cas != ["duo-retrait"]):
        p.error("La sonde de garde exige --phase3d --cas duo-retrait")
    if a.cas and not a.phase3d:
        p.error("--cas nécessite --phase3d")
    revision = git("rev-parse", "--verify", a.revision + "^{commit}").decode().strip()
    a.sortie.mkdir(parents=True, exist_ok=False)
    tmp = Path(os.environ["NOVAOS_TMPDIR"]).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="eth3-duo-src-", dir=tmp) as td:
        root = Path(td)
        with subprocess.Popen(["git", "-C", NOVA.parent, "archive", revision, "novaOS", "novaFS", "components"],
                              stdout=subprocess.PIPE) as proc:
            with tarfile.open(fileobj=proc.stdout, mode="r|") as tar:
                tar.extractall(path=root, filter="data")
            if proc.wait() != 0:
                raise ValueError("Extraction de la révision")
        sources = {}
        for name in MODULES:
            path = Path("novaOS/kernel/src") / name
            blob = git("show", f"{AUDIT}:{path}")
            dest = root / path
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(blob)
            reference = git("rev-parse", f"{AUDIT}:{path}").decode().strip()
            if git("hash-object", dest).decode().strip() != reference:
                raise ValueError(f"Module audité divergent {name}")
            sources[str(path)] = reference
        raccord = {}
        if a.phase3d:
            patch = root / "novaOS/tests/ethernet_v1/retrait_duo.patch"
            subprocess.run(["git", "apply", "--check", str(patch)], cwd=root, check=True)
            subprocess.run(["git", "apply", str(patch)], cwd=root, check=True)
            if a.sonder_garde:
                sonde = root / "novaOS/tests/ethernet_v1/sonde_garde_q35.patch"
                subprocess.run(["git", "apply", "--check", str(sonde)], cwd=root, check=True)
                subprocess.run(["git", "apply", str(sonde)], cwd=root, check=True)
            raccord = {name: git("hash-object", root / name).decode().strip() for name in sources}
            raccord["patch_sha256"] = hashlib.sha256(patch.read_bytes()).hexdigest()
        if a.phase4f:
            dynamique = root / "novaOS/tests/gestionnaire_4f/raccord_duo.patch"
            appliquer_raccord(root, dynamique, {
                "hardware/block/net_event.rs", "hardware/vtd_lab/duo.rs", "memory/mod.rs", "sched/process/nvme.rs",
            })
            continuite = None
            if a.phase4f in {"gestionnaire-actif", "lanceur-actif"}:
                if a.cas != ["port"]:
                    raise ValueError("Le témoin de vacance exige le profil de stockage nominal")
                continuite = root/"novaOS/tests/gestionnaire_4f/continuite_nvme.patch"
                appliquer_raccord(root, continuite, {
                    "hardware/block/net_event.rs", "sched/process/nvme.rs", "sched/process/nvme/broker.rs",
                })
        # L'option des modules d'origine n'existe que dans le manifeste de cette copie.
        manifest = root / "novaOS/kernel/Cargo.toml"
        text = manifest.read_text()
        feature = 'net-q35-duo = ["nvme-lab", "net-q35-lab", "ethernet-v1-port"]'
        if 'net-q35-duo =' in text or text.count("[features]") != 1:
            raise ValueError("Déclaration Cargo ambiguë")
        manifest.write_text(text.replace("[features]", "[features]\n" + feature))
        env = dict(os.environ, CARGO_TARGET_DIR=str(root / "target"), TMPDIR=str(tmp) + "/")
        env["RUSTC"] = subprocess.check_output(["rustup", "which", "--toolchain", "stable", "rustc"], text=True).strip()
        result = dict(port=revision, audit=AUDIT, modules=sources, elf={},
                      rustc=subprocess.check_output([env["RUSTC"], "-V"], text=True).strip())
        result["raccord_3d"] = raccord
        features = "net-q35-duo"
        if a.phase4f:
            features += ",gestionnaire-4f"
            env["NOVAOS_4E1_CAS"] = a.phase4f
            env["NOVAOS_4D_REFUS"] = ""
            result["gestionnaire_4f"] = a.phase4f
            result["raccord_4f_sha256"] = hashlib.sha256(dynamique.read_bytes()).hexdigest()
            if continuite is not None:
                result["observation_cqe_4f"] = hashlib.sha256(continuite.read_bytes()).hexdigest()
            result["modules_4f"] = {name: git("hash-object", root / name).decode().strip() for name in sources}
        if a.sonder_garde:
            result["sonde_garde_sha256"] = hashlib.sha256(sonde.read_bytes()).hexdigest()
        for cas in a.cas or ("port", "deces"):
            env["NOVAOS_ETH3_CASE"] = cas
            with (a.sortie / f"{cas}-compilation.log").open("xb") as log:
                subprocess.run(["rustup", "run", "stable", "cargo", "build", "--offline", "--locked",
                                "--release", "--target", "x86_64-unknown-none", "--features", features],
                               cwd=root/"novaOS", env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            elf = a.sortie / f"{cas}.elf"
            shutil.copyfile(root/"target/x86_64-unknown-none/release/novaos", elf)
            result["elf"][cas] = hashlib.sha256(elf.read_bytes()).hexdigest()
            if a.phase4f:
                sys.path.insert(0, str(NOVA/"tests"))
                from gestionnaire_4d.qualifier import elf as segments
                (out,) = [p.parent for p in (root/"target/x86_64-unknown-none/release").glob("build/novaos-*/out/gestionnaire.elf")]
                result.setdefault("images_4f", {})[cas] = {
                    "noyau": segments(elf),
                    "acteurs": {n:segments(out/n) for n in ("gestionnaire.elf", "pair.elf", "net_preparation.bin",
                                                            "ethernet_port.bin", "ethernet_client.bin")},
                    "paquet": hashlib.sha256((out/"pilote.pkg01").read_bytes()).hexdigest(),
                }
        (a.sortie / "sources.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
