"""Sélection explicite d’un modèle de laboratoire, empreinte vérifiée avant démarrage."""
import json
import gzip
import hashlib
import os
from pathlib import Path
import subprocess

from virtio_net_confinement import digest, dump


def executable(arch, habituel, sortie, *, acpi_pio=False):
    cle = {"aarch64": "NOVAOS_QEMU_RX_ARM", "x86_64": "NOVAOS_QEMU_RX_Q35"}[arch]
    fichier = os.environ.get(cle)
    if fichier is None:
        if acpi_pio:
            raise ValueError("Manifeste du modèle ACPI PIO absent")
        return habituel
    dossier = Path(fichier).resolve(strict=True)
    info = json.loads(dossier.read_text())
    binaire = Path(info["executable"]).resolve(strict=True)
    attendu = {"aarch64": "10.0.12", "x86_64": "11.1.3"}[arch]
    if info["cible"] != arch or info["version"] != attendu or binaire.name != "qemu-system-" + arch:
        raise ValueError("Modèle QEMU d’une autre version ou architecture")
    patch_pio = info.get("diff_acpi_pio")
    if (acpi_pio != (patch_pio is not None) or acpi_pio and (arch != "x86_64" or "diff_faute" in info or
            patch_pio != "ef953de95eaaad688f89bd49acf71c7bac7b75b594b5af5b8408dffbbd7cd3a9")):
        raise ValueError("Modèle ACPI PIO utilisé hors de son profil")
    mode = os.environ.get("NOVAOS_NET_MODELE_FAUTE")
    injection = os.environ.get("NOVAOS_NET_FAUTE_RX")
    if "diff_faute" in info:
        with gzip.open(Path(__file__).with_name("virtio_net_faute_rx.diff.gz"), "rb") as f:
            attendu_faute = hashlib.file_digest(f, "sha256").hexdigest()
        if (info["diff_faute"] != attendu_faute
                or (mode, injection) not in (("revue", None), ("injecter", "completion-15"))):
            raise ValueError("Modèle de faute hors du parcours expérimental")
    elif mode or injection:
        raise ValueError("Le parcours expérimental exige un modèle distinct")
    wait = os.environ.get("NOVAOS_Q35_WAIT")
    revue = os.environ.get("NOVAOS_Q35_REVUE_WAIT")
    if "diff_wait" in info:
        with gzip.open(Path(__file__).with_name("virtio_net_vtd_wait.diff.gz"),"rb") as stream:
            expected_wait=hashlib.file_digest(stream,"sha256").hexdigest()
        if (not acpi_pio or info["diff_wait"] != expected_wait or
            (revue, wait) not in (("1", None), (None, "court"), (None, "absent"), (None, "tardif"))):
            raise ValueError("Modèle WAIT hors du parcours Q35 expérimental")
    elif wait is not None or revue is not None:
        raise ValueError("L'injection WAIT exige un binaire dédié")
    empreinte = digest(binaire)
    version = subprocess.check_output([binaire, "--version"], text=True).strip()
    if (empreinte != info["sha256"] or version != info["version_executee"]
            or info["diff"] != "64bba00421bea605521de0514a5639730fa61fee2e1de2ee55480608bec832ef"):
        raise ValueError("Identité du modèle RX incorrecte")
    dump(sortie / "modele-rx.json", dict(executable=str(binaire), version=version, sha256=empreinte,
                                       construction_sha256=digest(dossier), correctif=info["diff"],
                                       correctif_faute=info.get("diff_faute"), mode_faute=mode, correctif_acpi_pio=patch_pio, correctif_wait=info.get("diff_wait"), mode_wait=wait))
    return str(binaire)
