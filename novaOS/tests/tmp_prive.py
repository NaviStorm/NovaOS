"""Qualification QEMU du service tmp sur une image NovaFS P9 jetable."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import select
import signal
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from bloc_qemu import command
from session_cli import fixture_p9


def capture(arch, sector, image, iso, scenario, log, vcpus):
    env = {**os.environ, "NOVA_ARCH": arch, "NOVA_ISO": str(iso), "NOVA_VCPUS": str(vcpus)}
    controls = ["-display", "none", "-monitor", "none", "-serial", "stdio"]
    argv = command(arch, sector, image, controls=controls)
    cut = scenario == "tmp-coupure"
    done = {"tmp-test": "TMP isolation, thread, enfants, arrêt forcé et faute vérifiés",
            "tmp-reprise": "TMP reprise vérifiée",
            "tmp-coupure": "TMP coupure prête fichier synchronisé handle ouvert"}[scenario].encode()
    with log.open("xb") as evidence, subprocess.Popen(
            argv, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, bufsize=0) as vm:
        data = bytearray()
        exited = False
        try:
            deadline = time.monotonic() + 150
            while time.monotonic() < deadline:
                if not select.select([vm.stdout], [], [], 1)[0]:
                    continue
                block = os.read(vm.stdout.fileno(), 16384)
                if not block:
                    break
                evidence.write(block)
                evidence.flush()
                data.extend(block)
                if b"Panique du noyau" in data:
                    break
                if done in data and cut:
                    vm.kill()
                    assert vm.wait(timeout=10) == -signal.SIGKILL
                    return bytes(data)
                if done in data and b"nsh> " in data and not exited:
                    # Ne pas laisser des acquittements du service manquer à l'arrêt.
                    vm.stdin.write(b"exit\n")
                    exited = True
                if exited and b"BLOC session terminee ok=true" in data and re.search(rb"SMP arret motif=normal[^\n]*\n", data):
                    return bytes(data)
        finally:
            if vm.poll() is None:
                vm.kill()
            vm.wait(timeout=10)
        raise RuntimeError(f"{log} : {data[-1000:]!r}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--secteur", type=int, choices=(512, 4096), required=True)
    parser.add_argument("--vcpus", type=int, choices=(1, 2, 4), required=True)
    parser.add_argument("--preuves", type=Path, required=True)
    args = parser.parse_args()
    arch = os.environ["NOVA_ARCH"]
    proofs = args.preuves.resolve()
    proofs.mkdir(parents=True, exist_ok=False)
    target = "aarch64-apple-darwin"
    fs = ROOT.parent / "novaFS"
    subprocess.run([fs / "lab/run", "build", "--offline", "--lib", "--bin", "novafs"],
                   env={**os.environ, "NOVAFS_TARGET": target}, check=True)
    lib = fs / "target" / target / "release"
    rows = []
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as working:
        work = Path(working)
        target_os = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}[arch]
        source = Path(os.environ.get("NOVA_KERNEL_ELF", ROOT / "target" / target_os / "release/novaos"))
        kernel = work / "noyau.elf"
        shutil.copyfile(source, kernel)
        kernel_sha = hashlib.sha256(kernel.read_bytes()).hexdigest()
        qemu = subprocess.check_output([f"qemu-system-{arch}", "--version"], text=True).splitlines()[0]
        oracle = work / "lecteur"
        subprocess.run(["rustup", "run", "stable", "rustc", "--edition=2024",
                        "--target", target, "-L", "dependency=" + str(lib / "deps"),
                        "--extern", "novafs=" + str(lib / "libnovafs.rlib"),
                        Path(__file__).with_name("tmp_prive_oracle.rs"), "-o", oracle], check=True)
        image = work / "disque.raw"
        fixture_p9(image, args.secteur)
        catalogue = [oracle, image, str(args.secteur), "catalogue"]
        baseline = subprocess.check_output(catalogue)
        (proofs / "catalogue-public.txt").write_bytes(baseline)
        for seq, scenario in enumerate(("tmp-test", "tmp-reprise", "tmp-coupure", "tmp-reprise")):
            iso = work / (scenario + ".iso")
            if not iso.exists():
                subprocess.run(["sh", ROOT / "scripts/iso.sh", scenario, iso], check=True,
                               env={**os.environ, "NOVA_KERNEL_ELF": str(kernel)}, stdout=subprocess.DEVNULL)
            label = f"{seq}-{scenario}"
            log = proofs / (label + ".serial")
            data = capture(arch, args.secteur, image, iso, scenario, log, args.vcpus)
            boot = re.search(rb"TMP d\xc3\xa9marrage ([0-9a-f]{32})", data)
            incarnation = re.search(rb"TMP (?:premi\xc3\xa8re|nouvelle) instance=(\d+) g\xc3\xa9n\xc3\xa9ration=(\d+)", data)
            assert boot and incarnation
            if scenario != "tmp-coupure":
                mask = hex((1 << args.vcpus) - 1)
                smp = f"attendus={mask} en_ligne={mask} ack={mask} absents=0x0 muets=0x0".encode()
                assert smp in data, f"arrêt SMP incomplet pour {args.vcpus} vCPU"
            if seq == 3:
                assert b"TMP reprise orphelins retires" in data
            state = "coupure" if seq == 2 else "propre"
            raw = image.read_bytes()
            archive = proofs / (label + ".raw.gz")
            with gzip.open(archive, "wb", compresslevel=6) as out:
                out.write(raw)
            with gzip.open(archive, "rb") as saved:
                assert saved.read() == raw
            check = subprocess.check_output([oracle, image, str(args.secteur), state], text=True).strip()
            assert baseline == subprocess.check_output(catalogue), "le catalogue public a changé"
            assert raw == image.read_bytes()
            rows.append(dict(scenario=scenario, qemu=qemu, architecture=arch, secteur=args.secteur,
                             vcpus=args.vcpus, noyau_sha256=kernel_sha,
                             iso_sha256=hashlib.sha256(iso.read_bytes()).hexdigest(), boot=boot[1].decode(),
                             instance=int(incarnation[1]), generation=int(incarnation[2]),
                             catalogue_sha256=hashlib.sha256(baseline).hexdigest(),
                             oracle=check, disque_sha256=hashlib.sha256(raw).hexdigest(),
                             journal_sha256=hashlib.sha256(log.read_bytes()).hexdigest()))
            print(f"{arch}/{args.secteur}/{args.vcpus} {label} {check}", flush=True)
        assert len({r["boot"] for r in rows}) == 4
        for old, new in ((rows[0], rows[1]), (rows[2], rows[3])):
            assert (old["instance"], old["generation"]) == (new["instance"], new["generation"])
        (proofs / "resultats.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
