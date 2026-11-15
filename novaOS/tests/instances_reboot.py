"""Identité réinjectée à travers un reset QMP puis la mort du processus QEMU."""
import hashlib
import json
import os
from pathlib import Path
import re
import select
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
CURRENT = b"INSTANCE reboot courante=OK travailleur=41 identite_stable=OK"
SOURCE = re.compile(rb"^BOOTID source=([^\s]+)\r?$", re.M)


def make_cd(iso, scenario, kernel):
    result = subprocess.run(["sh", ROOT / "scripts/iso.sh", scenario, iso],
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            env={**os.environ, "NOVA_KERNEL_ELF": str(kernel)})
    if result.returncode:
        raise RuntimeError(result.stdout.decode(errors="replace"))


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


def observe(vm, log, offset, source, previous=None, *, forced=False):
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
        if (ident and BANNER in text and CURRENT in text
                and (previous is None or rejection in text)):
            assert SOURCE.findall(text) == [source.encode()], text[-2000:]
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


def reset_and_wait(vm):
    """Attendre aussi l'événement RESET avant de relire la RAM de démarrage."""
    vm.stdin.write(b'{"execute":"system_reset"}\n')
    replied = reset = False
    deadline = time.monotonic() + 10
    while (remaining := deadline - time.monotonic()) > 0:
        if not select.select([vm.stdout], [], [], remaining)[0]:
            break
        msg = json.loads(vm.stdout.readline())
        if "error" in msg:
            raise RuntimeError(msg["error"])
        replied |= "return" in msg
        reset |= msg.get("event") == "RESET"
        if reset and replied:
            return
    raise RuntimeError("Réponse et événement RESET attendus de QEMU")


def fdt_bootid(vm, work):
    """Lire le DTB physique de QEMU virt avant EFI. Ne conserver aucune graine."""
    fdt = work / "dtb-avant-efi.bin"
    try:
        # Le profil virt 256 Mio place son DTB de 1 Mio au début de la RAM.
        qmp(vm, "pmemsave", val=0x40000000, size=1024 * 1024, filename=str(fdt))
        names = subprocess.check_output(["fdtget", "-p", fdt, "/chosen"]).splitlines()
        if b"rng-seed" not in names:
            return None
        raw = subprocess.check_output(["fdtget", "-t", "bx", fdt, "/chosen", "rng-seed"])
        seed = bytes(int(part, 16) for part in raw.split())
        assert len(seed) == 32
        return hashlib.sha256(b"NovaOS/BootID/v1/QEMU-ArmVirt/chosen/rng-seed\0"
                              + seed).digest()[:16].hex()
    finally:
        fdt.unlink(missing_ok=True)


def without_entropy(arch, iso, log, work):
    if arch == "x86_64":
        settings, extra = {"NOVA_X86_CPU": "max,rdrand=off,rdseed=off"}, ()
    else:
        settings = {"NOVA_ARM_CPU": "cortex-a72"}
        extra = ("-machine", "dtb-randomness=off", "-S")
    with machine(iso, log, settings=settings, extra=extra) as vm:
        if arch == "aarch64":
            assert fdt_bootid(vm, work) is None, "La source DT est encore présente"
            qmp(vm, "cont")
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
    cpu = os.environ.get(cpu_var, "cortex-a72" if arch == "aarch64" else "max")
    expected_source = {
        ("aarch64", "cortex-a72"): "UEFI-QEMU-RNG-SEED",
        ("aarch64", "max"): "RNDR",
        ("x86_64", "max"): "RDRAND",
    }[arch, cpu]
    profile = {cpu_var: cpu}
    BUILD.mkdir(exist_ok=True)
    # Le répertoire de preuve est unique même pour deux campagnes concurrentes.
    evidence = Path(tempfile.mkdtemp(prefix=f"instances-reboot-{arch}-", dir=BUILD))
    print(f"Journaux {evidence}", flush=True)
    warm_log, cold_log = evidence / "reset.serial", evidence / "relance.serial"
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as work:
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
        denied = without_entropy(arch, initial_cd, denied_log, directory)
        logs = [warm_log, cold_log, denied_log]
        check_fdt = arch == "aarch64" and cpu == "cortex-a72"
        pause = ("-S",) if check_fdt else ()
        derived = []
        with machine(initial_cd, warm_log, settings=profile, extra=pause) as vm:
            if check_fdt:
                derived.append(fdt_bootid(vm, directory))
                qmp(vm, "cont")
            first = observe(vm, warm_log, 0, expected_source)
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
            if check_fdt:
                qmp(vm, "stop")
            reset_and_wait(vm)
            if check_fdt:
                derived.append(fdt_bootid(vm, directory))
                qmp(vm, "cont")
            second = observe(vm, warm_log, offset, expected_source, first, forced=True)
            vm.kill()
            assert vm.wait(timeout=10) == -signal.SIGKILL

        make_cd(final_cd, "instances-reboot=" + second, kernel)
        with machine(final_cd, cold_log, settings=profile, extra=pause) as restarted:
            if check_fdt:
                derived.append(fdt_bootid(restarted, directory))
                qmp(restarted, "cont")
            third = observe(restarted, cold_log, 0, expected_source, second)
            qmp(restarted, "quit")
            assert restarted.wait(timeout=10) == 0

    assert len({identity[:32] for identity in (first, second, third)}) == 3
    if check_fdt:
        assert derived == [ident[:32] for ident in (first, second, third)], derived
    report = {
        "architecture": arch, "vcpus": 1,
        "cpu": cpu, "elf_sha256": kernel_sha,
        "sources": [s.decode() for p in (warm_log, cold_log) for s in SOURCE.findall(p.read_bytes())],
        "qemu": subprocess.check_output([f"qemu-system-{arch}", "--version"],
                                        text=True).splitlines()[0],
        "identites": [first, second, third],
        "transitions": ["arrêt normal invité puis system_reset QMP", "SIGKILL QEMU puis relance"],
        "resultats_transitions": [
            {"ancienne_refusee": old, "courante_acceptee": current,
             "instance_generation_identiques": old[32:] == current[32:],
             "admission": "même principal, même manifeste worker_image",
             "argument_travailleur": 3, "code_travailleur": 41,
             "identite_relue_apres_travailleur": "identique"}
            for old, current in ((first, second), (second, third))
        ],
        "sans_entropie": denied,
        "journaux_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                            for p in logs},
    }
    if check_fdt:
        report["bootid_recalcules_depuis_dtb_physique"] = derived
    (evidence / "resultats.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(f"Trois démarrages, deux identités antérieures refusées — {arch}", flush=True)


if __name__ == "__main__":
    run()
