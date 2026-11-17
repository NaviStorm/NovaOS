"""Recette du vrai ELF (catalogue SDK) et de la documentation du vrai nsh."""

import argparse
import base64
from contextlib import ExitStack
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import struct
import subprocess as sp
import sys
import tarfile
import tempfile
import fcntl
import time
from types import SimpleNamespace

from host import HERE, NOVA, execute
from bilan import measures

ROOT = HERE.parent
OS = ROOT / "novaOS"
sys.path.insert(0, str(OS / "tests"))
from cli import Terminal
from bloc import qmp
from bloc_qemu import command
from journalisation import attend, sortie
from pipe_natif import fixture

TARGETS = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def rows(path):
    for r in json.loads(path.read_text())["cas"]:
        if (
            r["locale"] != "C"
            or r["posix"]
            or r["cas"].startswith("sortie-lecture-seule-")
            or r["cas"] in ("tube-sans-lecteur", "silence-tube-fermé")
        ):
            continue
        args = list(map(bytes.fromhex, r["argv"]))
        ref = r["nova"] if r["documentation"] else r["gnu"]
        code = ref["rc"]
        out = bytes.fromhex(ref["stdout"])
        err = bytes.fromhex(ref["stderr"]).replace(b"gsleep:", b"sleep:")
        try:
            for arg in args:
                arg.decode()
        except UnicodeDecodeError:
            code, out, err = 3, b"", b""
        cancel = r["demande_s"] is None
        yield r["cas"], args, 6, (125 if cancel else code, out, err), r["demande_s"]
    for flag in ("--help", "--version"):
        result = execute(NOVA, [flag], loc="fr")
        yield (
            flag + "-fr",
            [flag.encode(), b"--", b"nsh-doc=fr"],
            6,
            (0, bytes.fromhex(result["stdout"]), b""),
            0,
        )
    yield (
        "sortie-refusée",
        [b"--help"],
        4,
        (1, b"", "sleep: accès refusé\n".encode()),
        0,
    )
    yield (
        "version-refusée",
        [b"--version"],
        4,
        (1, b"", "sleep: accès refusé\n".encode()),
        0,
    )
    yield "aucun-port", [b".002"], 0, (0, b"", b""), 0.002
    for arg in ["0", ".001", ".005", ".020", ".100"]:
        for i in range(8):
            yield f"mesure-{arg}-{i}", [arg.encode()], 0, (0, b"", b""), float(arg)


def prepare(work, arch, vectors):
    # Seuls le contrôleur d'essai et son observateur sont copiés/modifiés.
    # Les sources de production et les points d'entrée de nsh restent ceux du worktree.
    tree = work / "src"
    tree.mkdir()
    for name in ["components", "novaFS"]:
        (tree / name).symlink_to(ROOT / name, target_is_directory=True)
    kernel = tree / "novaOS"
    kernel.mkdir()
    for p in OS.iterdir():
        if p.name == "kernel":
            shutil.copytree(p, kernel / p.name, ignore=shutil.ignore_patterns("target"))
        elif p.name not in ("target", "build"):
            (kernel / p.name).symlink_to(p, target_is_directory=p.is_dir())
    source = kernel / "kernel/src/sched/process/ipc/thread_probe.rs"
    text = source.read_text()
    begin = text.index("pub(crate) fn run_example()")
    end = text.index("pub(crate) fn run()", begin)
    source.write_text(
        text[:begin] + (HERE / "lab/controleur.rs").read_text() + "\n" + text[end:]
    )
    buf = bytearray()
    for _, args, ports, _, delay in vectors:
        argv = bytes([len(args)]) + b"".join(
            struct.pack("<H", len(a)) + a for a in args
        )
        data = struct.pack("<5Q", 0, 0, 0, 0, len(argv)) + argv
        buf += struct.pack("<HBB", len(data), ports, delay is None) + data
    source.with_name("sleep-argv.bin").write_bytes(buf)
    p = kernel / "kernel/src/sched/process/syscall.rs"
    text = p.read_text()
    anchor = "    LINE_OPEN.store(!text.ends_with('\\n'), Ordering::Relaxed);"
    assert text.count(anchor) == 1
    p.write_text(
        text.replace(
            anchor,
            """    for octets in bytes.chunks(16) {
        hal::log(format_args!("SLEEPHEX {} {right} {:02x?}",pid.number,octets));
    }
"""
            + anchor,
        )
    )
    sched = kernel / "kernel/src/sched/mod.rs"
    anchor = "profile::tick(cpu, user, old == Self::idle_slot(cpu));"
    contents = sched.read_text()
    assert contents.count(anchor) == 1
    sched.write_text(
        contents.replace(
            anchor,
            anchor
            + """
                if self.thread(old).owner.is_some() {
                    process::ipc::thread_probe::TICKS_ENFANT.fetch_add(1, Ordering::Relaxed);
                }""",
        )
    )
    elf = HERE / f"bin/sleep-{arch}.elf"
    cache = work / "cible"
    env = dict(os.environ, CARGO_TARGET_DIR=str(cache), NOVA_THREAD_PROBE_ELF=str(elf))
    sp.run(
        [
            "rustup",
            "run",
            "stable",
            "cargo",
            "build",
            "--offline",
            "--release",
            "--locked",
            "--target",
            TARGETS[arch],
            "--features",
            "thread-probe",
        ],
        cwd=kernel,
        env=env,
        check=True,
    )
    image = cache / TARGETS[arch] / "release/novaos"
    assert elf.read_bytes() in image.read_bytes()
    return image


def iso_check(iso, kernel, work):
    extracted = work / (iso.stem + "-extrait.elf")
    sp.run(
        [
            "xorriso",
            "-osirrox",
            "on",
            "-indev",
            iso,
            "-extract",
            "/boot/novaos.elf",
            extracted,
        ],
        check=True,
        stdout=sp.DEVNULL,
        stderr=sp.DEVNULL,
    )
    assert sha(extracted) == sha(kernel)
    return {"iso": sha(iso), "noyau": sha(extracted)}


def sdk(arch, sector, work, image, kernel, vectors):
    iso = work / "sdk.iso"
    env = dict(os.environ, NOVA_ISO=str(iso), NOVA_KERNEL_ELF=str(kernel))
    sp.run(
        ["sh", OS / "scripts/iso.sh", "sdk-example", iso],
        env=env,
        check=True,
        stdout=sp.DEVNULL,
    )
    media = iso_check(iso, kernel, work)
    argv = command(
        arch,
        sector,
        image,
        readonly=True,
        controls=[
            "-display",
            "none",
            "-monitor",
            "none",
            "-serial",
            "file:" + str(work / "sdk.serial"),
        ],
    )
    before = sha(image)
    with (
        (work / "qemu.log").open("wb") as err,
        sp.Popen(argv, env=env, stdin=sp.DEVNULL, stdout=err, stderr=err) as vm,
    ):
        try:
            log = work / "sdk.serial"
            end = time.monotonic() + 300
            while time.monotonic() < end and vm.poll() is None:
                time.sleep(0.25)
                if not log.exists():
                    continue
                text = log.read_bytes()
                if b"Panique du noyau" in text:
                    raise RuntimeError(text[-1000:])
                if f"SLEEPTERM {len(vectors)}".encode() in text:
                    break
            else:
                raise TimeoutError("recette SDK incomplète")
        finally:
            vm.kill()
            vm.wait()
    log = log.read_bytes()
    results = []
    for i, (key, args, ports, expected, mode) in enumerate(vectors):
        start = re.search(rf"^SLEEPBEGIN {i}\r?$".encode(), log, re.M)
        finish = re.search(
            rf"^SLEEPEND {i} pid=(\d+) Exit\((\d+)\) (\d+) (\d+) (\d+)".encode(),
            log,
            re.M,
        )
        assert start and finish, (i, key, log[-500:])
        frames = re.findall(
            rb"SLEEPHEX (\d+) ([24]) \[([0-9a-f, ]+)\]",
            log[start.end() : finish.start()],
        )
        out, err = bytearray(), bytearray()
        for pid, port, data in frames:
            assert pid == finish[1]
            (out if port == b"2" else err).extend(
                bytes.fromhex(data.decode().replace(",", ""))
            )
        got = int(finish[2]), out, err
        seconds = int(finish[3]) / int(finish[4])
        timing = (
            not key.startswith(("mesure-", "durée-")) or mode <= seconds <= mode + 0.250
        )
        results.append(
            dict(
                id=key,
                argv=[a.hex() for a in args],
                ports=ports,
                attendu=expected[0],
                statut=got[0],
                ok=got == expected and timing,
                demande_s=mode,
                temps_s=seconds,
                compteur=int(finish[3]),
                hz=int(finish[4]),
                cpu_ticks=int(finish[5]),
                stdout=out.hex(),
                stderr=err.hex(),
                pid=int(finish[1]),
            )
        )
    assert before == sha(image)
    return dict(
        media=media,
        elf=sha(HERE / f"bin/sleep-{arch}.elf"),
        p9=before,
        commande=argv,
        cas=results,
    )


def shell(arch, sector, work, image):
    kernel = OS / "target" / TARGETS[arch] / "release/novaos"
    os.environ["NOVA_KERNEL_ELF"] = str(kernel)
    results = []
    with ExitStack() as stack:
        a, b = socket.socketpair()
        stack.enter_context(a)
        stack.enter_context(b)
        channel = stack.enter_context(a.makefile("rwb", buffering=0))
        monitor = SimpleNamespace(stdin=channel, stdout=channel)
        vm = stack.enter_context(
            Terminal(
                arch,
                sector,
                image,
                "sleep",
                journal=work / "nsh.serial",
                qmp_bloc=b.fileno(),
            )
        )
        b.close()
        qmp(monitor, "qmp_capabilities")
        vm.until(b"nsh> ")
        attend(
            lambda: sortie(vm, "service status 16"),
            lambda s: "Current state: RUNNING" in s,
        )

        def state():
            (stats,) = [
                r["stats"]
                for r in qmp(monitor, "query-blockstats")
                if r["device"] == "d"
            ]
            return [
                sha(image),
                stats["wr_bytes"],
                stats["wr_operations"],
                stats["flush_operations"],
            ]

        for _ in range(30):
            before = state()
            time.sleep(0.5)
            if before == state():
                break
        else:
            raise RuntimeError("volume actif")
        for tag, word in [
            ("en-US", "Wait for the sum"),
            ("fr-CA", "Attendre la somme"),
            ("fr--CA", "Wait for the sum"),
        ]:
            vm.line("export LANG=" + tag)
            for args in ["--help", "0 --h", "--version", "--v"]:
                response = sortie(vm, "sleep " + args)
                assert (
                    "nova-cli 0.20.0" if args.startswith("--v") else word
                ) in response
                results.append([tag, args, 0])
            vm.vm.stdin.write(b"man sleep; echo sleepman=$?\n")
            frame = vm.until(b"q quitter" if tag == "fr-CA" else b"q quit  / search")
            assert word.encode() in frame
            vm.vm.stdin.write(b"q")
            vm.until(b"sleepman=0")
            vm.until(b"nsh> ")
            results.append([tag, "man sleep", 0])
        vm.line("export LANG=en")
        for name in ("help", "version"):
            for i in range(1, len(name) + 1):
                flag = "--" + name[:i]
                for cmd, code in [
                    ("sleep " + flag, 0),
                    ("sleep x " + flag, 0),
                    ("sleep " + flag + " -x", 0),
                    ("sleep " + flag + "=x", 1),
                    ("sleep -x " + flag, 1),
                ]:
                    response = sortie(vm, cmd, code)
                    if code == 0:
                        assert (
                            "Wait for the sum" if name == "help" else "nova-cli 0.20.0"
                        ) in response
                    else:
                        assert "Try 'sleep --help'" in response
                    results.append(["en", cmd, code])
        after = state()
        assert before == after
        for cmd in [
            "sleep 0",
            "sleep .002 0s",
            "/system/bin/sleep 0",
            "sleep -- --help",
            "sleep --",
        ]:
            sortie(vm, cmd, 127)
            results.append(["nsh", cmd, 127])
        vm.vm.stdin.write(b"nsh -c 'sleep 0 | head -n 1'; echo sleeppipe=$?\n")
        vm.until(b"sleeppipe=3")
        vm.until(b"nsh> ")
        results.append(["nsh", "sleep 0 | head -n 1", 3])
    return dict(
        media=iso_check(work / "nsh.iso", kernel, work),
        cas=results,
        avant=before,
        apres=after,
    )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=TARGETS)
    p.add_argument("profil", choices=["sdk", "nsh"])
    p.add_argument(
        "--out",
        type=Path,
        default=Path(
            os.environ.get("NOVAOS_TMPDIR", HERE.parent.parent / "tmp/sleep-arc027")
        )
        / "rejeu",
    )
    p.add_argument("--vecteurs", type=Path, default=HERE / "preuves/options.json")
    p.add_argument("--rapide", action="store_true")
    p.add_argument("--mesures", action="store_true")
    a = p.parse_args()
    tmp = Path(
        os.environ.get("NOVAOS_TMPDIR", ROOT.parent / "tmp/sleep-arc027")
    ).resolve()
    tmp.mkdir(exist_ok=True, parents=True)
    os.environ.update(
        NOVAOS_TMPDIR=str(tmp),
        TMPDIR=str(tmp) + "/",
        NOVA_ARCH=a.arch,
        NOVA_RAM="256",
        NOVA_VCPUS="1",
        RUSTC=sp.check_output(
            ["rustup", "which", "--toolchain", "stable", "rustc"], text=True
        ).strip(),
    )
    work = Path(tempfile.mkdtemp(prefix=f"sleep-{a.profil}-{a.arch}-", dir=tmp))
    a.out.mkdir(parents=True, exist_ok=True)
    print(work, flush=True)
    vectors = list(rows(a.vecteurs))
    if a.mesures:
        vectors = [v for v in vectors if v[0].startswith("mesure-")]
        if a.rapide:
            vectors = [v for v in vectors if int(v[0].rsplit("-", 1)[1]) < 4]
    elif a.rapide:
        vectors = [
            v
            for v in vectors
            if v[0]
            in (
                "0",
                "--h",
                "--v",
                "aucun",
                "aucun-port",
                "sortie-refusée",
                "arrêt-inf",
                "durée-0.003",
                "syntaxe-1",
            )
        ]
    kernel = prepare(work, a.arch, vectors) if a.profil == "sdk" else None
    lock = (tmp / "arc027-mesures.lock").open("a+")
    fcntl.flock(lock, fcntl.LOCK_EX)
    for sector in (512,) if a.rapide else (512, 4096):
        cell = work / str(sector)
        cell.mkdir()
        image, _, _ = fixture(cell, sector, {"temoin": b"ARC-027\n"})
        shutil.copyfile(image, cell / "p9-initial.raw")
        result = (
            sdk(a.arch, sector, cell, image, kernel, vectors)
            if kernel
            else shell(a.arch, sector, cell, image)
        )
        name = f"{a.profil}-{a.arch}-{sector}"
        for ext, src in [
            ("json", json.dumps(result, indent=2, ensure_ascii=False).encode()),
            ("serial", (cell / f"{a.profil}.serial").read_bytes()),
        ]:
            with gzip.open(a.out / f"{name}.{ext}.gz", "wb") as f:
                f.write(src)
        media = [cell / (a.profil + ".iso"), cell / "p9-initial.raw", image]
        if (cell / "nsh.vm.json").exists():
            media.append(cell / "nsh.vm.json")
        archive = a.out / f"media-{name}.tar.xz"
        with tarfile.open(archive, "w:xz") as tar:
            for path in media:
                tar.add(path, arcname=path.name)
        with tarfile.open(archive, "r:xz") as tar:
            for path in media:
                assert hashlib.sha256(
                    tar.extractfile(path.name).read()
                ).hexdigest() == sha(path)
        failed = [r for r in result["cas"] if isinstance(r, dict) and not r["ok"]]
        print(
            name,
            len(result["cas"]),
            "écarts",
            len(failed),
            [r["id"] for r in failed[:10]],
            flush=True,
        )
        if failed:
            raise AssertionError("écarts ELF")
        if kernel:
            for arg, n, median, p95, cpu, peak in measures(result["cas"]):
                print(
                    f"{name} sleep {arg}: n={n}, médiane {median:.3f} ms, retard p95 {p95:.3f} ms, CPU {cpu}/{peak} ticks médiane/max"
                )


if __name__ == "__main__":
    main()
