"""Éprouver l'ELF livré, le moteur et le vrai nsh sur deux géométries P9."""

import argparse
import hashlib
import json
import os
import re
import socket
from contextlib import ExitStack
from types import SimpleNamespace
import statistics
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
TMP = ROOT.parent / "tmp/printf-0240"
sys.path.insert(0, str(ROOT / "novaOS/tests"))
from bloc import qmp
from cli import Terminal
from journalisation import arret
from session_cli import fixture_p9
from bloc_qemu import command
from threads import TARGETS


def run(args, cwd=ROOT, **env):
    subprocess.run(args, cwd=cwd, env={**os.environ, **env}, check=True)


def sha(p):
    with p.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def prepare(arch, work, vectors):
    target = TARGETS[arch]
    cargo = [
        "rustup",
        "run",
        "stable",
        "cargo",
        "build",
        "--release",
        "--offline",
        "--locked",
        "--target",
        target,
    ]
    env = dict(
        RUSTC=subprocess.check_output(
            ["rustup", "which", "--toolchain", "stable", "rustc"], text=True
        ).strip(),
        CARGO_TARGET_DIR=str(work / "target"),
    )
    run(
        cargo + ["--features", "sonde"],
        ROOT / "components/printf",
        **env,
        PRINTF_VECTORS=str(vectors.resolve()),
        RUSTFLAGS="-Crelocation-model=static -Cno-redzone=yes -Clink-arg=--strip-all -Clink-arg=-zmax-page-size=4096 -Clink-arg=-T"
        + str(ROOT / "novaOS/user/novafs/service.ld"),
    )
    probe = work / "sonde.elf"
    shutil.copyfile(work / "target" / target / "release/printf", probe)
    # Compile the existing sdk-example scenario in an isolated tree, adding
    # test invocations only to its test controller. All syscall handlers,
    # the shared launcher and the delivered ELF are byte-identical inputs.
    src = work / "sources"
    for folder in ["novaOS", "novaFS", "components"]:
        shutil.copytree(
            ROOT / folder,
            src / folder,
            ignore=shutil.ignore_patterns("target", "build", "__pycache__"),
        )
    # Some source trees legitimately contain directories named "build".
    tracked = subprocess.check_output(
        ["git", "ls-files", "-z", "novaOS", "novaFS", "components"], cwd=ROOT
    )
    for entry in tracked.split(b"\0"):
        if entry:
            rel = Path(os.fsdecode(entry))
            dest = src / rel
            if not dest.exists():
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / rel, dest)
    controller = src / "novaOS/kernel/src/sched/process/ipc/thread_probe.rs"
    old = controller.read_text()
    key = "pub(crate) fn run_example() {"
    assert old.count(key) == 1
    controller.write_text(
        old.replace(
            key,
            (HERE / "elf-recette.rs").read_text()
            + "\n"
            + key
            + "\n    exercice_elf();",
        )
    )
    elf = HERE / "bin" / f"printf-{arch}.elf"
    run(
        cargo + ["--features", "thread-probe"],
        src / "novaOS",
        **env,
        NOVA_THREAD_PROBE_ELF=str(probe),
        PRINTF_ELF=str(elf.resolve()),
    )
    kernel = work / "target" / target / "release/novaos"
    image = kernel.read_bytes()
    assert probe.read_bytes() in image and elf.read_bytes() in image
    iso = work / "sdk.iso"
    run(
        ["sh", ROOT / "novaOS/scripts/iso.sh", "sdk-example", iso],
        NOVA_KERNEL_ELF=str(kernel),
        NOVA_ARCH=arch,
    )
    readback = work / "noyau-sdk.elf"
    run(
        [
            "xorriso",
            "-osirrox",
            "on",
            "-indev",
            iso,
            "-extract",
            "/boot/novaos.elf",
            readback,
        ]
    )
    assert sha(readback) == sha(kernel)
    return iso, probe, readback


def run_sdk(arch, sector, iso, disk, serial, count):
    env = {**os.environ, "NOVA_ARCH": arch, "NOVA_ISO": str(iso)}
    cmd = command(
        arch,
        sector,
        disk,
        readonly=True,
        controls=["-display", "none", "-monitor", "none", "-serial", "stdio"],
    )
    with serial.open("wb") as log:
        vm = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, env=env)
        try:
            deadline = time.monotonic() + 100
            while time.monotonic() < deadline and vm.poll() is None:
                trace = serial.read_bytes()
                if b"Panique du noyau" in trace:
                    raise ValueError(trace[-1500:])
                if b"SDK exemple admis sortie=0 restitution=OK" in trace:
                    break
                time.sleep(0.15)
            else:
                raise RuntimeError(serial.read_bytes()[-2000:])
        finally:
            vm.kill()
            vm.wait()
    assert f"PRINTF MOTEUR cas={count} ecarts=0".encode() in trace, trace[-1800:]
    texts = [
        b"Nova/000f",
        b"+1.12|0x1.8p-2",
        b"a",
        b"value not completely converted",
        b"Manual: man printf",
        b"printf (NovaOS) 0.24.0",
        b"warning: ignoring excess",
        b"write error",
        b"write error",
        b"write error",
        b"write error",
        b"",
        b"invalid conversion specification",
    ]
    for i, expected in enumerate(texts):
        segment = trace.split(f"PRINTF ELF debut={i} ".encode(), 1)[1].split(
            f"PRINTF ELF fin={i} ".encode(), 1
        )[0]
        assert expected in segment, (i, segment)
    perf = {}
    for mode, label in enumerate(["court", "texte", "binaire", "décimal"], 1):
        rows = [
            tuple(map(int, m))
            for m in re.findall(
                rf"PRINTF PERF mode={mode} pass=(\d+) cycles=(\d+) hz=(\d+) appels=(\d+)".encode(),
                trace,
            )
        ]
        assert len(rows) == 6 and [row[0] for row in rows] == list(range(6)), rows
        median = statistics.median(
            cycles * 1e9 / (hz * n) for _, cycles, hz, n in rows[1:]
        )
        octets = [1, 30 * 49, 30 * 18, 30 * len(b"+98765.125\n")][mode - 1]
        perf[label] = dict(
            brut=rows,
            mediane_ns_par_appel=median,
            octets_par_appel=octets,
            octets_par_seconde=octets * 1e9 / median,
        )
    return dict(
        performance_moteur=perf,
        commande_qemu=cmd,
        cas_moteur=count,
        cas_elf=len(texts),
        serie_sdk=sha(serial),
    )


def shell(arch, sector, disk, out):
    serial = out / "shell.serial"
    records = []

    def call(vm, line, code=0, need=b""):
        vm.vm.stdin.write(f"{line}; echo __rc_$?__fin\n".encode())
        text = vm.until(f"__rc_{code}__fin".encode())
        vm.until(b"nsh> ")
        assert need in text, (line, need, text[-1000:])
        records.append(dict(commande=line, statut=code))

    with ExitStack() as stack:
        client, peer = socket.socketpair()
        stack.enter_context(client)
        stack.enter_context(peer)
        client.settimeout(15)
        channel = stack.enter_context(client.makefile("rwb", buffering=0))
        monitor = SimpleNamespace(stdin=channel, stdout=channel)
        vm = stack.enter_context(
            Terminal(
                arch, sector, disk, "printf", qmp_bloc=peer.fileno(), journal=serial
            )
        )
        peer.close()
        qmp(monitor, "qmp_capabilities")

        def counters():
            (stats,) = [
                b["stats"]
                for b in qmp(monitor, "query-blockstats")
                if b["device"] == "d"
            ]
            return {
                k: stats[k]
                for k in (
                    "wr_bytes",
                    "wr_operations",
                    "flush_operations",
                    "failed_wr_operations",
                    "failed_flush_operations",
                )
            }

        vm.until(b"nsh> ")
        call(vm, "true")
        # The service manager continues its boot transactions after the first
        # prompt. Start the observation only after a stable, bounded interval.
        deadline = time.monotonic() + 30
        before = sha(disk)
        since = time.monotonic()
        while time.monotonic() - since < 3:
            time.sleep(0.25)
            current = sha(disk)
            if current != before:
                before = current
                since = time.monotonic()
            if time.monotonic() > deadline:
                raise RuntimeError("P9 encore actif au repos")
        shutil.copyfile(disk, out / "avant-documentation.raw")
        counts_before = counters()
        for lang, fr in [("fr-CA", True), ("fr-zzzzz", False)]:
            call(vm, "export LANG=" + lang)
            call(
                vm,
                "printf --help",
                need=("Écrit des octets" if fr else "Write bytes").encode(),
            )
            call(vm, "printf --version", need=b"printf (NovaOS) 0.24.0")
            vm.vm.stdin.write(b"man printf; echo __man_$?__fin\n")
            page = vm.until(b"q quitter" if fr else b"q quit ")
            assert (
                "Écrire des octets" if fr else "Print formatted bytes"
            ).encode() in page
            vm.vm.stdin.write(b"q")
            vm.until(b"__man_0__fin")
            vm.until(b"nsh> ")
            records.append(dict(commande="man printf", langue=lang, statut=0))
        shutil.copyfile(disk, out / "apres-documentation.raw")
        after = sha(out / "apres-documentation.raw")
        counts_after = counters()
        assert counts_before == counts_after, (counts_before, counts_after)
        assert before == after, (before, after)
        for line in [
            "printf %s Nova",
            "printf --help x",
            "printf %s --version",
            "/system/bin/printf %d 7",
        ]:
            call(vm, line, 127)
        arret(vm, 2)
    return dict(
        shell=records,
        compteurs_avant=counts_before,
        compteurs_apres=counts_after,
        p9_avant_documentation=before,
        p9_apres_documentation=after,
        serie_shell=sha(serial),
        iso_shell=sha(serial.with_suffix(".iso")),
    )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "--sdk",
        type=Path,
        help="réutiliser les médias SDK déjà construits pour cette architecture",
    )
    p.add_argument("arch", choices=TARGETS)
    p.add_argument(
        "fonctionnel", type=Path, help="dossier créé par tester-options-printf.sh"
    )
    p.add_argument("sortie", type=Path)
    a = p.parse_args()
    TMP.mkdir(parents=True, exist_ok=True)
    os.environ.update(
        NOVAOS_TMPDIR=str(TMP),
        TMPDIR=str(TMP) + "/",
        NOVA_VCPUS="2",
        NOVA_RAM="256",
        PYTHONDONTWRITEBYTECODE="1",
    )
    work = Path(tempfile.mkdtemp(dir=TMP, prefix="invites-" + a.arch + "-"))
    out = a.sortie.resolve()
    out.mkdir(parents=True, exist_ok=False)
    (ROOT / "novaOS/build").mkdir(exist_ok=True)
    run(["novaFS/lab/run", "build", "--offline", "--bin", "novafs"])
    run(["make", "-C", "novaOS", "ARCH=" + a.arch, "kernel"])
    vectors = a.fonctionnel / "vecteurs.bin"
    records = json.loads((a.fonctionnel / "options.json").read_bytes())
    assert all(r["ok"] for r in records)
    count = sum("argv_hex" in r for r in records)
    if a.sdk:
        iso = a.sdk / "sdk.iso"
        probe = a.sdk / "sonde.elf"
        kernel = work / "noyau-sdk.elf"
        run(
            [
                "xorriso",
                "-osirrox",
                "on",
                "-indev",
                iso,
                "-extract",
                "/boot/novaos.elf",
                kernel,
            ]
        )
        binary = kernel.read_bytes()
        assert (
            probe.read_bytes() in binary and vectors.read_bytes() in probe.read_bytes()
        )
        assert (HERE / "bin" / f"printf-{a.arch}.elf").read_bytes() in binary
    else:
        iso, probe, kernel = prepare(a.arch, work, vectors)
    for path in (iso, probe, kernel):
        shutil.copy2(path, out / path.name)
    for sector in (512, 4096):
        cell = out / str(sector)
        cell.mkdir()
        disk = cell / "session.raw"
        fixture_p9(disk, sector)
        initial = sha(disk)
        shutil.copyfile(disk, cell / "genese.raw")
        sdk = run_sdk(a.arch, sector, iso, disk, cell / "sdk.serial", count)
        assert sha(disk) == initial
        result = shell(a.arch, sector, disk, cell)
        (cell / "resultat.json").write_text(
            json.dumps(
                dict(
                    architecture=a.arch,
                    secteur=sector,
                    genese=initial,
                    iso_sdk=sha(iso),
                    sonde=sha(probe),
                    noyau_sdk=sha(kernel),
                    vecteurs=sha(vectors),
                    elf=sha(HERE / "bin" / f"printf-{a.arch}.elf"),
                    **sdk,
                    **result,
                ),
                indent=2,
                ensure_ascii=False,
            )
            + "\n"
        )
        print(a.arch, sector, "OK", flush=True)


if __name__ == "__main__":
    main()
