"""Identité réinjectée à travers un reset QMP puis la mort du processus QEMU."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
import time
from contextlib import contextmanager

from demarrage import BANNER, BUILD, ROOT, qmp

IDENTITY = re.compile(rb"^INSTANCE reboot identite=([0-9a-f]{64})\r?$", re.M)
HALTED = b"SMP arret motif=normal"
FINISHED = b"INSTANCE reboot termine=OK"


def make_cd(iso, scenario, kernel):
    subprocess.run(["sh", ROOT / "scripts/iso.sh", scenario, iso], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                   env={**os.environ, "NOVA_KERNEL_ELF": str(kernel)})


@contextmanager
def machine(iso, log, *, settings=None, extra=()):
    command = ["sh", ROOT / "scripts/qemu.sh", "-display", "none", "-monitor", "none",
               "-serial", f"file:{log}", "-qmp", "stdio", *extra]
    with subprocess.Popen(command, env={**os.environ, **(settings or {}),
                                        "NOVA_ISO": str(iso), "NOVA_VCPUS": "1"},
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0) as vm:
        try:
            qmp(vm, "qmp_capabilities")
            yield vm
        finally:
            if vm.poll() is None:
                vm.kill()
            vm.wait(timeout=10)


def observe(vm, log, offset, previous=None, *, forced=False):
    deadline = time.monotonic() + 90
    text = b""
    if previous is not None:
        rejection = b"INSTANCE reboot ancienne=" + previous.encode() + b" refusee=OK"
    while time.monotonic() < deadline and vm.poll() is None:
        if log.exists():
            text = log.read_bytes()[offset:]
        if b"Panique du noyau" in text:
            break
        ident = IDENTITY.search(text)
        if ident and BANNER in text and (previous is None or rejection in text):
            current = ident[1].decode()
            if previous is not None:
                # Le même processus témoin est créé au même rang à chaque boot.
                # La seule différence de la requête rejetée doit être le BootID.
                assert current[32:] == previous[32:], (current, previous)
                assert current[:32] != previous[:32], "BootID réutilisé"
            if forced:
                assert FINISHED not in text and HALTED not in text
                return current
            if FINISHED in text and HALTED in text:
                return current
        time.sleep(0.025)
    raise RuntimeError(text[-2200:].decode(errors="replace"))


def without_entropy(arch, iso, log, *, stock=False):
    if arch == "x86_64":
        settings, extra = {"NOVA_X86_CPU": "max,rdrand=off,rdseed=off"}, ()
    else:
        settings = {"NOVA_ARM_CPU": "cortex-a72"}
        extra = () if stock else ("-machine", "dtb-randomness=off")
    with machine(iso, log, settings=settings, extra=extra) as vm:
        deadline = time.monotonic() + 90
        text = b""
        while time.monotonic() < deadline and vm.poll() is None:
            if log.exists():
                text = log.read_bytes()
            if HALTED in text or b"Panique du noyau" in text:
                break
            time.sleep(0.025)
        assert BANNER in text and HALTED in text, text[-1800:]
        assert b"INSTANCE reboot sans_entropie identite_refusee=OK historique=OK" in text, text[-1800:]
        assert b"BOOTID source=indisponible" in text and IDENTITY.search(text) is None
        qmp(vm, "quit")
        assert vm.wait(timeout=10) == 0
    return {"cpu": next(iter(settings.values())), "arguments_supplementaires": list(extra),
            "identite_durable": "refusée", "chemin_historique": "réussi"}


def run():
    arch = os.environ["NOVA_ARCH"]
    cpu_var = "NOVA_ARM_CPU" if arch == "aarch64" else "NOVA_X86_CPU"
    cpu = os.environ.get(cpu_var, "max")
    profile = {cpu_var: cpu}
    BUILD.mkdir(exist_ok=True)
    # Le répertoire de preuve est unique même pour deux campagnes concurrentes.
    evidence = Path(tempfile.mkdtemp(prefix=f"instances-reboot-{arch}-", dir=BUILD))
    print(f"Journaux {evidence}", flush=True)
    warm_log, cold_log = evidence / "reset.serial", evidence / "relance.serial"
    with tempfile.TemporaryDirectory(dir=BUILD) as work:
        directory = Path(work)
        initial_cd, forced_cd, final_cd = [directory / f"{n}.iso" for n in (1, 2, 3)]
        target = subprocess.check_output(["sh", "-c", '. ./scripts/target.sh && echo "$rust_target"'],
                                         cwd=ROOT, text=True).strip()
        kernel = directory / "novaos.elf"
        shutil.copyfile(os.environ.get("NOVA_KERNEL_ELF", ROOT / "target" / target / "release/novaos"),
                        kernel)
        kernel_sha = hashlib.sha256(kernel.read_bytes()).hexdigest()
        make_cd(initial_cd, "instances-reboot", kernel)
        denied_log = evidence / "absence.serial"
        denied = without_entropy(arch, initial_cd, denied_log)
        logs = [warm_log, cold_log, denied_log]
        if arch == "aarch64":
            stock_log = evidence / "cortex-a72.serial"
            stock = without_entropy(arch, initial_cd, stock_log, stock=True)
            logs.append(stock_log)
        with machine(initial_cd, warm_log, settings=profile) as vm:
            first = observe(vm, warm_log, 0)
            # Le support est en lecture seule. Seule la ligne Limine change.
            make_cd(forced_cd, "instances-coupure=" + first, kernel)
            drives = qmp(vm, "query-block")
            cd, = [d for d in drives if d.get("inserted", {}).get("file") == str(initial_cd)]
            qmp(vm, "blockdev-change-medium", device=cd["device"], filename=str(forced_cd),
                format="raw")
            # qemu.sh termine la VM sur reset par défaut. L'essai exige le
            # maintien du processus QEMU et de la NVRAM durant cette transition.
            qmp(vm, "set-action", reboot="reset")
            offset = warm_log.stat().st_size
            qmp(vm, "system_reset")
            second = observe(vm, warm_log, offset, first, forced=True)
            vm.kill()
            assert vm.wait(timeout=10) == -signal.SIGKILL

        make_cd(final_cd, "instances-reboot=" + second, kernel)
        with machine(final_cd, cold_log, settings=profile) as restarted:
            third = observe(restarted, cold_log, 0, second)
            qmp(restarted, "quit")
            assert restarted.wait(timeout=10) == 0

    assert len({identity[:32] for identity in (first, second, third)}) == 3
    sources = re.findall(rb"^BOOTID source=(\w+)\r?$",
                         warm_log.read_bytes() + cold_log.read_bytes(), re.M)
    assert len(sources) == 3 and all(x != b"indisponible" for x in sources)
    report = {
        "architecture": arch, "vcpus": 1,
        "cpu": cpu, "elf_sha256": kernel_sha,
        "sources": [s.decode() for s in sources],
        "qemu": subprocess.check_output([f"qemu-system-{arch}", "--version"],
                                        text=True).splitlines()[0],
        "identites": [first, second, third],
        "transitions": ["arrêt normal invité puis system_reset QMP", "SIGKILL QEMU puis relance"],
        "identites_refusees": [first, second],
        "sans_entropie": denied,
        "journaux_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                            for p in logs},
    }
    if arch == "aarch64":
        report["profil_ordinaire"] = stock
    (evidence / "resultats.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(f"Trois démarrages, deux identités antérieures refusées — {arch}", flush=True)


if __name__ == "__main__":
    run()
