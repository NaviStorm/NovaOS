"""Refus d’admission de nsh sur une image jetable, avant son premier prompt."""
import argparse
import hashlib
import gzip
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from tmp_prive import ROOT, command, fixture_p9
from session_cli import fixture


def exercise(arch, image, sector, iso, log, kind):
    env = {**os.environ, "NOVA_ARCH": arch, "NOVA_ISO": str(iso), "NOVA_VCPUS": "1"}
    controls = ["-display", "none", "-monitor", "none", "-serial", "file:" + str(log)]
    trace = log.with_suffix(".bloc")
    controls += ["-trace", "enable=virtio_blk_handle_*,file=" + str(trace)]
    if kind == "sans-bootid":
        if arch == "aarch64":
            env["NOVA_ARM_CPU"] = "cortex-a72"
            controls += ["-machine", "dtb-randomness=off"]
        else:
            env["NOVA_X86_CPU"] = "max,rdrand=off,rdseed=off"
    initial = image.read_bytes()
    digest = hashlib.sha256(initial).hexdigest()
    archive = log.with_suffix(".initiale.raw.gz")
    with gzip.open(archive, "xb") as saved:
        saved.write(initial)
    with subprocess.Popen(command(arch, sector, image, controls=controls), env=env) as vm:
        try:
            deadline = time.monotonic() + 90
            while time.monotonic() < deadline:
                output = log.read_bytes() if log.exists() else b""
                if b"SMP arret motif=normal" in output or vm.poll() is not None:
                    break
                time.sleep(0.05)
        finally:
            if vm.poll() is None:
                vm.terminate()
            vm.wait(timeout=10)
    output = log.read_bytes()
    assert b"BLOC session terminee ok=false" in output and b"SMP arret motif=normal" in output, output[-1500:]
    assert b"nsh> " not in output and b"Panique du noyau" not in output
    if kind == "sans-bootid":
        assert b"BLOC admission refusee 12" in output
    io = trace.read_bytes()
    assert b"virtio_blk_handle_write" not in io
    if kind == "p8-sur-lanceur-p9":
        assert b"TMP format P8 non pris en charge" in output
        assert b"service=Exit(25)" in output and b"client=Exit(25)" in output
        assert b"virtio_blk_handle_read" in io
    if kind == "p9-sur-lanceur-p8":
        assert b"CLI historique P8 refuse le format P9" in output
        assert b"virtio_blk_handle_read" in io
    with gzip.open(archive, "rb") as saved:
        assert saved.read() == initial == image.read_bytes()
    return dict(cas=kind, architecture=arch, secteur=sector, iso_sha256=hashlib.sha256(iso.read_bytes()).hexdigest(), image_avant_apres=digest, ecritures_qemu=0,
                trace=hashlib.sha256(output).hexdigest(), bloc_sha256=hashlib.sha256(io).hexdigest())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preuves", type=Path, required=True)
    parser.add_argument("--secteur", type=int, choices=(512, 4096), default=512)
    parser.add_argument("--production", action="store_true", help="Lancer cli et cli-p8 sans le droit QUALIFY")
    args = parser.parse_args()
    arch = os.environ["NOVA_ARCH"]
    proofs = args.preuves.resolve()
    proofs.mkdir(parents=True, exist_ok=False)
    rows = []
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp:
        iso = Path(tmp) / "cli.iso"
        suffix = "" if args.production else "-test"
        subprocess.run(["sh", ROOT / "scripts/iso.sh", "cli" + suffix, iso], check=True)
        legacy = Path(tmp) / "p8.iso"
        subprocess.run(["sh", ROOT / "scripts/iso.sh", "cli-p8" + suffix, legacy], check=True)
        for kind in ("sans-bootid", "volume-invalide", "p8-sur-lanceur-p9", "p9-sur-lanceur-p8"):
            disk = Path(tmp) / (kind + ".raw")
            if kind in ("sans-bootid", "p9-sur-lanceur-p8"):
                fixture_p9(disk, args.secteur)
            elif kind == "p8-sur-lanceur-p9":
                fixture(disk, args.secteur)
            else:
                with disk.open("xb") as out:
                    out.truncate(16 * 1024 * 1024)
            medium = legacy if kind == "p9-sur-lanceur-p8" else iso
            rows.append(exercise(arch, disk, args.secteur, medium, proofs / (kind + ".serial"), kind))
    (proofs / "resultats.json").write_text(json.dumps(rows, indent=2) + "\n")


if __name__ == "__main__":
    main()
