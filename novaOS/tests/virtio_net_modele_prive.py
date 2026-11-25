"""Sélection explicite d’un modèle de laboratoire, empreinte vérifiée avant démarrage."""
import json
import os
from pathlib import Path
import subprocess

from virtio_net_confinement import digest, dump


def executable(arch, habituel, sortie):
    cle = {"aarch64": "NOVAOS_QEMU_RX_ARM", "x86_64": "NOVAOS_QEMU_RX_Q35"}[arch]
    fichier = os.environ.get(cle)
    if fichier is None:
        return habituel
    dossier = Path(fichier).resolve(strict=True)
    info = json.loads(dossier.read_text())
    binaire = Path(info["executable"]).resolve(strict=True)
    attendu = {"aarch64": "10.0.12", "x86_64": "11.1.3"}[arch]
    if info["cible"] != arch or info["version"] != attendu or binaire.name != "qemu-system-" + arch:
        raise ValueError("Modèle QEMU d’une autre version ou architecture")
    empreinte = digest(binaire)
    version = subprocess.check_output([binaire, "--version"], text=True).strip()
    if (empreinte != info["sha256"] or version != info["version_executee"]
            or info["diff"] != "64bba00421bea605521de0514a5639730fa61fee2e1de2ee55480608bec832ef"):
        raise ValueError("Identité du modèle RX incorrecte")
    dump(sortie / "modele-rx.json", dict(executable=str(binaire), version=version, sha256=empreinte,
                                       construction_sha256=digest(dossier), correctif=info["diff"]))
    return str(binaire)
