"""Coupure du processus QEMU aux barrières P9 et remontage du même disque en lecture seule."""
if not __debug__:
    raise RuntimeError("Les contrôles de qualification exigent les assertions Python.")

import argparse
import json
import os
from pathlib import Path
import re
import tempfile
import time
from types import SimpleNamespace

from virtio_net_disque_arm import HOST, PATTERN, ROOT, digest, dump, firmware_files, image, lanceurs, observe, oracle, qualification, tpm, verifier_gpt


def extrait(disk, s, ids, dest, taille):
    offset = verifier_gpt(disk, s, ids)["p9"] * s
    with disk.open("rb") as src, dest.open("xb") as target:
        src.seek(offset)
        target.write(src.read(taille))


def campagne(elf, sources, secteur, phase, out):
    out.mkdir(parents=True, exist_ok=False)
    vol = sources / str(secteur) / "initial.p9"
    initial = sources / str(secteur) / "migration.json"
    if digest(vol) != json.loads(initial.read_text())["sha256"]:
        raise ValueError("Image de référence modifiée")
    import subprocess
    with vol.open("rb") as source:
        inspect = subprocess.check_output([HOST, "inspect", "512", str(secteur)], stdin=source)
    volumes = set(re.findall(rb" LV=(\w+)", inspect))
    if len(volumes) != 1:
        raise ValueError("Identité de volume ambiguë")
    disk = out / "systeme.raw"
    ids = image(disk, vol, elf, secteur, scenario="net-arm-p9", volume_id=bytes.fromhex(volumes.pop().decode()))
    report = {"secteur": secteur, "phase": phase, "elf_sha256": digest(elf), "volume_initial": digest(vol)}
    try:
        with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"], prefix="net") as td:
            tmp = Path(td)
            lanceurs(tmp)
            report["distribution"] = firmware_files(tmp)
            for nom in (phase, "remonter-ro"):
                dossier = out / nom
                dossier.mkdir()
                horloge = {}
                report[nom] = horloge
                with qualification(ROOT.parent.parent, horloge):
                    start = time.monotonic()
                    try:
                        with tpm(tmp, dossier, "labo") as sock:
                            ok = observe(tmp, dossier, sock, disque=disk, secteur=secteur, phase=nom)
                    finally:
                        horloge["execution_s"] = time.monotonic()-start
                if not ok:
                    raise ValueError(f"Point invité absent pour {nom}")
                part = dossier / "relecture.p9"
                extrait(disk, secteur, ids, part, vol.stat().st_size)
                horloge["volume_sha256"] = digest(part)
                bench = SimpleNamespace(image=part, sector=secteur)
                oracle(bench, None)
                if nom == phase:
                    candidats = {"avant-f2": (b"ancien", PATTERN), "apres-f2": (PATTERN,)}.get(nom, (b"ancien",))
                    for attendu in candidats:
                        try:
                            horloge["generation"] = oracle(bench, attendu)
                            report["issue"] = "ancien" if attendu == b"ancien" else "nouveau"
                            break
                        except AssertionError:
                            continue
                    else:
                        raise ValueError("Le volume ne correspond à aucune issue autorisée")
                else:
                    if horloge["volume_sha256"] != report[phase]["volume_sha256"] or \
                            ("FICHIERS oracle " + report["issue"]) not in (dossier/"serie.log").read_text(errors="replace"):
                        raise ValueError("Le remontage à froid ne confirme pas l'oracle")
            report["succes"] = True
    finally:
        dump(out/"campagne.json", report)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for k in ("elf", "sources", "sortie"):
        p.add_argument(k, type=lambda s: Path(s).resolve())
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("phase", choices=("avant-f1", "apres-f1", "avant-f2", "apres-f2"))
    a = p.parse_args()
    campagne(a.elf, a.sources, a.secteur, a.phase, a.sortie)
