"""nsh ordinaire, moteur dans cp de recette et admission SDK de l’ELF livré."""

import argparse
import base64
import contextlib
import fcntl
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import select
import shutil
import subprocess as sp
import sys
import time
import socket
import tempfile
from types import SimpleNamespace
from options import HERE, SOURCE, GNU

ROOT = HERE.parent
sys.path.insert(0, str(ROOT / "novaOS/tests"))
from cli import Terminal
from pipe_natif import fixture, host
from journalisation import sortie, attend
from bloc import qmp
from bloc_qemu import command

TARGETS = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}


def sha(p):
    with p.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def vec(data):
    return 'b"' + "".join(f"\\x{x:02x}" for x in data) + '"'


def prepare(arch, lab, log):
    target = TARGETS[arch]
    for name in ("components", "novaFS", "novaOS"):
        shutil.copytree(
            ROOT / name,
            lab / name,
            dirs_exist_ok=True,
            ignore=shutil.ignore_patterns("target", "build", "__pycache__"),
        )
    cp = lab / "components/coreutils/cp.rs"
    shutil.copyfile(HERE / "temoin.rs", cp)
    core = cp.parent
    (core / "dd-temoin.bin").write_bytes(SOURCE)
    rows = json.load(gzip.open(HERE / "preuves/options.json.gz"))["rows"]
    vectors = []
    ids = []
    for i, r in enumerate(rows):
        args = r["args"]
        g = r["gnu"]
        if r["pipe_fragmente"] or not r["ok"] or r["lang"] != "C":
            continue
        if any(
            w.startswith(
                ("if=", "of=", "seek=", "oseek=", "iflag=", "oflag=", "--h", "--v")
            )
            for w in args
        ):
            continue
        if any(
            "fsync" in w or "fdatasync" in w or w == "bs=100M" or w == "bs=10M"
            for w in args
        ):
            continue
        expr = "None" if g["rc"] else "Some(" + vec(base64.b64decode(g["stdout"])) + ")"
        vectors.append(
            "(&[" + ",".join(vec(w.encode()) for w in args) + "]," + expr + ")"
        )
        ids.append(i)
    (core / "dd-vecteurs.rs").write_text("&[\n" + ",\n".join(vectors) + "\n]")
    compiler = sp.check_output(
        ["rustup", "which", "--toolchain", "stable", "rustc"], text=True
    ).strip()
    env = dict(
        os.environ, CARGO_TARGET_DIR=str(lab / "target"), RUSTC=compiler, NOVA_ARCH=arch
    )
    with log.open("w") as f:
        sp.run(
            ["make", "-C", str(lab / "novaOS"), "ARCH=" + arch, "kernel"],
            env=env,
            stdout=f,
            stderr=sp.STDOUT,
            check=True,
        )
    kernel = lab / "target" / target / "release/novaos"
    lab_kernel = lab / "moteur.elf"
    shutil.copy2(kernel, lab_kernel)
    # Le témoin SDK lance l’ELF de livraison inchangé. Il ne lui délègue aucun fichier.
    recipe = lab / "novaOS/kernel/src/sched/process/ipc/thread_probe.rs"
    source = recipe.read_text()
    start = source.index("    let app = spawn::launch_thread_witness(&[])")
    end = source.index("    assert!(with(", start)
    recipe.write_text(
        source[:start]
        + (HERE / "sdk.rs").read_text()
        + "    probes::dd_streams(); probes::dd_matrix();\n"
        + source[end:]
    )
    probes = recipe.with_name("probes.rs")
    runpy.run_path(str(HERE / "sdk-matrice.py"))["generate"](
        probes.parent, log.parent / "cas-elf.json"
    )
    with probes.open("a") as f:
        for src in ("sdk-flux.rs", "sdk-exact.rs"):
            f.write((HERE / src).read_text())
    env["NOVA_THREAD_PROBE_ELF"] = str(HERE / "bin" / ("dd-" + arch + ".elf"))
    with log.open("a") as f:
        sp.run(
            [
                "rustup",
                "run",
                "stable",
                "cargo",
                "build",
                "--release",
                "--offline",
                "--locked",
                "-p",
                "novaos",
                "--features",
                "thread-probe",
                "--target",
                target,
            ],
            cwd=lab / "novaOS",
            env=env,
            stdout=f,
            stderr=sp.STDOUT,
            check=True,
        )
    assert Path(env["NOVA_THREAD_PROBE_ELF"]).read_bytes() in kernel.read_bytes()
    return ids, lab_kernel, kernel


def sdk(arch, sector, out, image, kernel):
    iso = out / "sdk.iso"
    env = dict(
        os.environ, NOVA_KERNEL_ELF=str(kernel), NOVA_ISO=str(iso), NOVA_VCPUS="1"
    )
    sp.run(
        ["sh", ROOT / "novaOS/scripts/iso.sh", "sdk-example", iso], env=env, check=True
    )
    argv = command(
        arch,
        sector,
        image,
        readonly=True,
        controls=["-display", "none", "-monitor", "none", "-serial", "stdio"],
    )
    before = sha(image)
    p = sp.Popen(
        argv, env=env, stdin=sp.DEVNULL, stdout=sp.PIPE, stderr=sp.STDOUT, bufsize=0
    )
    data = bytearray()
    try:
        deadline = time.monotonic() + 240
        while time.monotonic() < deadline:
            if not select.select([p.stdout], [], [], 1)[0]:
                continue
            b = os.read(p.stdout.fileno(), 65536)
            if not b:
                break
            data.extend(b)
            with (out / "sdk.serial").open("ab") as stream:
                stream.write(b)
            if (
                b"SDK exemple admis sortie=0 restitution=OK" in data
                or b"Panique du noyau" in data
            ):
                break
    finally:
        p.kill()
        p.wait()
        (out / "sdk.serial").write_bytes(data)
    assert (
        before == sha(image)
        and b"SDK exemple admis sortie=0 restitution=OK" in data
        and data.count(b"DD-ELF") == 7
        and data.count(b"DD-FLUX") == 3
        and data.count(b"DD-RESULTAT")
        == len(json.loads((out / "cas-elf.json").read_text()))
    ), data[-1400:]
    return dict(
        disque=before,
        elf=sha(HERE / "bin" / ("dd-" + arch + ".elf")),
        noyau=sha(kernel),
        cmd=argv,
    )


def run(arch, sector, out):
    out.mkdir(parents=True, exist_ok=False)
    os.environ.update(NOVA_ARCH=arch, NOVA_VCPUS="2", NOVA_RAM="256")
    prod = ROOT / "novaOS/target" / TARGETS[arch] / "release/novaos"
    sp.run(["make", "-C", ROOT / "novaOS", "ARCH=" + arch, "kernel"], check=True)
    if not (ROOT / "novaFS/target/aarch64-apple-darwin/release/novafs").exists():
        sp.run(
            [ROOT / "novaFS/lab/run", "build", "--offline", "--bin", "novafs"],
            check=True,
        )
    lab = Path(
        tempfile.mkdtemp(dir=os.environ["TMPDIR"], prefix="dd-qemu-" + arch + "-")
    )
    ids, kernel, sdk_kernel = prepare(arch, lab, out / "build.log")
    source = bytes(range(256)) * 16
    image, fs, lv = fixture(
        out, sector, {"entree": source, "zeros": bytes(len(source))}
    )
    os.environ["NOVA_KERNEL_ELF"] = str(prod)
    docs = []
    with contextlib.ExitStack() as stack:
        sock, child = socket.socketpair()
        stack.enter_context(sock)
        stack.enter_context(child)
        stream = stack.enter_context(sock.makefile("rwb", buffering=0))
        monitor = SimpleNamespace(stdin=stream, stdout=stream)
        vm = stack.enter_context(
            Terminal(
                arch,
                sector,
                image,
                "dd",
                journal=out / "nsh.serial",
                qmp_bloc=child.fileno(),
            )
        )
        child.close()
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
            return [sha(image), stats["wr_operations"], stats["flush_operations"]]

        for _ in range(30):
            before = state()
            time.sleep(0.5)
            if before == state():
                break
        for tag, fr in [("fr-CA", True), ("fr--CA", False), ("en-US", False)]:
            vm.line("export LANG=" + tag)
            for opt, marker in [
                ("--help", "Copier" if fr else "Copy"),
                ("--he", "Copier" if fr else "Copy"),
                ("--version", "nova-dd"),
                ("--ver", "nova-dd"),
            ]:
                assert marker in sortie(vm, "dd " + opt)
                docs.append([tag, opt, 0])
            vm.vm.stdin.write(b"man dd; echo manuel=$?\n")
            frame = vm.until(b"q quitter" if fr else b"q quit  / search")
            assert (b"Copier" if fr else b"Copy") in frame
            vm.vm.stdin.write(b"q")
            vm.until(b"manuel=0")
            vm.until(b"nsh> ")
            docs.append([tag, "man dd", 0])
        after = state()
        assert before == after
        rc_nom = int(
            sortie(vm, "dd if=entree of=sortie-dd status=none", (0, 1, 127))
            .split("journalrc=")[-1]
            .strip()
        )
        sortie(vm, "test -e sortie-dd", int(rc_nom != 0))
        vm.vm.stdin.write(
            b"nsh -c 'cat entree | dd count=0 status=none'; echo piperc=$?\n"
        )
        vm.until(b"\r\npiperc=")
        rc_pipe = int(vm.until(b"\r\n").strip())
        vm.until(b"nsh> ")
        if rc_pipe not in (0, 1, 3, 127):
            raise ValueError(rc_pipe)
        sortie(vm, "test -e /dev/zero", 1)
    os.environ["NOVA_KERNEL_ELF"] = str(kernel)
    measures = []
    hashes = {}
    with Terminal(arch, sector, image, "dd-labo", journal=out / "moteur.serial") as vm:
        vm.until(b"nsh> ")
        assert f"DD-VECTEURS {len(ids)} OK" in sortie(vm, "cp entree matrice")
        assert "DD-FIN 67" in sortie(vm, "cp entree conversion")
        for i in range(4):
            for dest in (
                ["copie512", "copie4k"] if i % 2 == 0 else ["copie4k", "copie512"]
            ):
                vm.vm.stdin.write(f"cp zeros {dest}; echo ddok=$?\n".encode())
                vm.until(b"DD-DEBUT\r\n")
                begin = time.monotonic_ns()
                vm.until(b"DD-FIN 4096")
                ns = time.monotonic_ns() - begin
                vm.until(b"ddok=0")
                vm.until(b"nsh> ")
                measures.append(
                    dict(
                        bs=512 if dest == "copie512" else 4096,
                        passage=i,
                        ns=ns,
                        octets=4096,
                    )
                )
                if i < 3:
                    sortie(vm, "rm " + dest)
    anchors = re.findall(
        r"slot=(\d+) génération=(\d+)".encode(),
        host(image, "inspect", "512", str(sector)),
    )
    slot = max(anchors, key=lambda a: int(a[1]))[0]
    converted = sp.run(
        [GNU, "skip=2B", "count=67B", "ibs=7", "conv=swab,ucase", "status=none"],
        input=source,
        capture_output=True,
        check=True,
    ).stdout
    for name, expected in {
        "conversion": converted,
        "matrice": b"",
        "copie512": bytes(4096),
        "copie4k": bytes(4096),
        "entree": source,
        "zeros": bytes(4096),
    }.items():
        data = host(image, "salvage", "512", str(sector), fs, lv, slot, "/" + name)
        assert data == expected, name
        hashes[name] = hashlib.sha256(data).hexdigest()
    if rc_nom == 0:
        copied = host(image, "salvage", "512", str(sector), fs, lv, slot, "/sortie-dd")
        assert copied == source
        hashes["sortie-dd"] = hashlib.sha256(copied).hexdigest()
    observations = sdk(arch, sector, out, image, sdk_kernel)
    report = dict(
        arch=arch,
        secteur=sector,
        nsh=rc_nom,
        pipe_nsh=rc_pipe,
        docs=docs,
        avant_doc=before,
        apres_doc=after,
        ids_moteur=ids,
        mesures=measures,
        sha256=hashes,
        sdk=observations,
        noyau_ordinaire=sha(prod),
        noyau_recette=sha(kernel),
    )
    (out / "resultats.json").write_text(json.dumps(report, indent=2) + "\n")
    print(arch, sector, len(ids), "vecteurs et mesures P9 vérifiés", flush=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=TARGETS)
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("sortie", type=Path)
    a = p.parse_args()
    # Une recette peut être rejouée en parallèle dans une autre branche.
    with (Path(os.environ["NOVAOS_TMPDIR"]) / "arc027-mesures.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        run(a.arch, a.secteur, a.sortie.resolve())
