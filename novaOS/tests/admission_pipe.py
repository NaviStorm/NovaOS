"""Retrait du producteur dans l'invité et collecte par PROCESS_WAIT v1."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

from demarrage import BUILD, ROOT, boot


def qualify(arch, cpus, out):
    out.mkdir(parents=True, exist_ok=False)
    BUILD.mkdir(exist_ok=True)
    iso = out / "admission.iso"
    env = dict(os.environ, NOVA_ARCH=arch, NOVA_VCPUS=str(cpus))
    subprocess.run(["sh", ROOT / "scripts/iso.sh", "ipc-test", iso], env=env, check=True)
    emulator = "qemu-system-" + arch
    records = dict(architecture=arch, vcpu=cpus, iso_sha256=hashlib.sha256(iso.read_bytes()).hexdigest(),
                   qemu=subprocess.check_output([emulator, "--version"], text=True).splitlines()[0])

    def check(vm, directory, log):
        deadline = time.monotonic() + 90
        try:
            while vm.poll() is None and time.monotonic() < deadline:
                serial = log.read_text(errors="replace") if log.exists() else ""
                if "Panique du noyau" in serial or "Exception noyau" in serial:
                    raise RuntimeError(serial[-1300:])
                if "PIPE admission restitution OK" in serial:
                    assert f"PIPE admission vcpu={cpus} cpu_producteur={cpus - 1}" in serial
                    assert "PIPE admission reponse_perdue QUERY meme_wait apres_collecte=aucun acquittement=unique OK" in serial
                    assert "PIPE admission sortie_normale=125 force=0 slot_recycle=protege OK" in serial
                    assert "PIPE admission saturation offres=8 attente=1 collecte=125 OK" in serial
                    assert "PIPE admission parent_disparu orphelins=0 reponse_tardive=refusee OK" in serial
                    cases = re.findall(r"PIPE admission cas=([\w-]+) force=([01]) statut=(\d+) ticks=(\d+) temoin=actif", serial)
                    assert [c[0] for c in cases] == ["refus-elf", "sortie-normale", "course", "retrait-canal", "admis"]
                    for name, forced, status, ticks in cases:
                        assert int(ticks) < 250
                        if name in ("refus-elf", "retrait-canal"):
                            assert (forced, status) == ("1", "125")
                        if name in ("sortie-normale", "admis"):
                            assert forced == "0"
                    records["cas"] = cases
                    return
                time.sleep(0.05)
            raise TimeoutError("restauration des ressources non attestée")
        finally:
            if log.is_file():
                shutil.copyfile(log, out / "serie.log")

    boot(f"admission-pipe-{arch}-{cpus}cpu", check, NOVA_ARCH=arch, NOVA_VCPUS=str(cpus), NOVA_ISO=str(iso))
    (out / "resultats.json").write_text(json.dumps(records, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=("aarch64", "x86_64"))
    parser.add_argument("cpus", type=int, choices=(1, 2, 4))
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    qualify(args.arch, args.cpus, args.destination.resolve())
