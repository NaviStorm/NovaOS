"""Construit dans une extraction privée le modèle de lecture des acquittements TX."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from port_qemu import digest, qualification

MANIFESTE = "ce5d426fe1989fe3222f23e26aa97bea64e0d80d1afea27e0ee2f09d4f5ba08b"
ARCHIVE = "7c9605290b34152debb842e965a55d2e4fbba4793e536ab677b4027e5dc0ba1a"
TRANSPORT = "527beaf95cd492dc2725abfc8f0f7beb043c40b7bf9ccbe7bfabeec76842e1ad"
ENTETE = "1886c173e0364e5055ff0d7e7c7217b39570c1f4aae0db0e24fea3e9527c5828"


def construire(sortie):
    tmp = Path(os.environ["NOVAOS_TMPDIR"]).resolve(strict=True)
    historique = tmp.parent / "archives-virtio-net/rx-corrige-20261125"
    recette = historique / "compilation/10.0.12-utm-2/construction.json"
    archive = historique / "amont/qemu-10.0.12-utm.tar.xz"
    patch = Path(__file__).with_name("modele_reset_arm.patch")
    if digest(recette) != MANIFESTE or digest(archive) != ARCHIVE:
        raise ValueError("Empreinte de la recette ou de QEMU UTM")
    ref = json.loads(recette.read_text())
    sortie.mkdir(parents=True, exist_ok=False)
    modele = sortie / "qemu-system-aarch64"
    preuves = dict(archive=ARCHIVE, recette=MANIFESTE, patch=digest(patch), modele_reference=ref["sha256"])
    with qualification(tmp.parent, preuves), tempfile.TemporaryDirectory(dir=tmp, prefix="e3e-mmio-") as td:
        work = Path(td)
        subprocess.run(["tar", "-xf", archive, "-C", work], check=True)
        src = work / "qemu-10.0.12-utm"
        nic = src / "hw/net/virtio-net.c"
        historique_rx = recette.with_name("virtio-net-corrige.c")
        if (digest(nic) != ref["original"] or digest(historique_rx) != ref["corrige"]
                or digest(src / "hw/virtio/virtio-pci.c") != TRANSPORT
                or digest(src / "include/hw/virtio/virtio-pci.h") != ENTETE):
            raise ValueError("Sources réseau ou PCI différentes du témoin")
        shutil.copyfile(historique_rx, nic)
        subprocess.run(["patch", "-p1", "-F", "0", "-i", str(patch.resolve())], cwd=src, check=True)
        build = work / "build"
        build.mkdir()
        for nom, cmd in (("configure", [src/"configure", *ref["configure"][1:]]), ("ninja", ref["ninja"])):
            with (sortie/(nom+".log")).open("xb") as journal:
                subprocess.run(cmd, cwd=build, stdout=journal, stderr=subprocess.STDOUT, check=True)
        shutil.copy2(build/modele.name, modele)
        preuves.update(sha256=digest(modele), version=subprocess.check_output([modele,"--version"],text=True).strip())
    (sortie/"construction.json").write_text(json.dumps(preuves, indent=2)+"\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie", type=lambda s: Path(s).resolve())
    construire(p.parse_args().sortie)
