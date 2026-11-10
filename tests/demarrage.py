"""Démarrages UEFI sur la même image que make run pour chaque architecture."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import select
import subprocess
import tempfile
import time


BUILD = Path("build")
BANNER = b"NovaOS v0.5.1\r\n"
# Captures PPM intégrales vérifiées visuellement en 800 × 600.
SCREENS = {
    "demarrage-aarch64": "c9c743d33a6c14fe6c035c4a3b2fefd1d8f23017973209abe5985e390d6eb4cb",
    "demarrage": "9fea8ea19fa06fcbfab01bdcd0e0538b9a4be3bc819d1b1bc184a79cb97f91d5",
    "composition": "d137f834269c693372b498ca613e0a9271a92710698abe0af631ce3d7995e345",
    "frappe": "95f4f02b6b581c183f75825c6309452064d6ca55b21d6a2f0281e650c9f75154",
    "saisie": "0c88b713a6f62f945c7645756c0d3b0f2916ff868438744703a788e8b6343643",
}


def wait_for_serial(vm, log, expected):
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline and vm.poll() is None:
        if log.exists() and BANNER + expected.encode() + b"\r\n" in log.read_bytes():
            return
        time.sleep(0.05)
    raise RuntimeError(f"Diagnostic série attendu : {expected}")


def qmp(vm, command, **arguments):
    request = {"execute": command, "arguments": arguments}
    vm.stdin.write(json.dumps(request).encode() + b"\n")
    while select.select([vm.stdout], [], [], 5)[0]:
        line = vm.stdout.readline()
        if not line:
            break
        reply = json.loads(line)
        if "return" in reply:
            return reply["return"]
        if "error" in reply:
            raise RuntimeError(f"{command} : {reply['error']}")
        # L’accueil QMP et les événements asynchrones précèdent parfois la réponse.
    raise RuntimeError(f"Réponse QMP absente : {command}")


def key(vm, *keys):
    # Les qcodes décrivent les positions physiques, indépendamment de la langue hôte.
    events = []
    for down, group in ((True, keys), (False, reversed(keys))):
        for code in group:
            events.append({"type": "key", "data": {
                "down": down, "key": {"type": "qcode", "data": code},
            }})
    qmp(vm, "input-send-event", events=events)
    time.sleep(0.012)


def wait_for_halt(vm, interrupts_enabled):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        registers = qmp(vm, "human-monitor-command", **{"command-line": "info registers"})
        halted = re.search(r"\bRFL=([0-9a-fA-F]+)[^\n]*\bHLT=1\b", registers)
        if halted and bool(int(halted[1], 16) & 0x200) == interrupts_enabled:
            return
        time.sleep(0.02)
    raise RuntimeError("Le processeur n’a pas atteint la veille avec l’état IF attendu.")


def expect_screen(vm, directory, name, timeout=5):
    ppm = directory / "courant.ppm"
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        qmp(vm, "screendump", filename=str(ppm))
        frame = ppm.read_bytes()
        if hashlib.sha256(frame).hexdigest() == SCREENS[name]:
            qmp(vm, "screendump", filename=str(BUILD / f"{name}.png"), format="png")
            return frame
        time.sleep(0.03)
    qmp(vm, "screendump", filename=str(BUILD / "echec.png"), format="png")
    raise RuntimeError(f"Écran différent de {name}. Voir build/echec.png.")


def test_keyboard(vm, directory):
    initial = expect_screen(vm, directory, "demarrage")
    wait_for_halt(vm, True)
    for chord in [("bracket_left",), ("e",), ("shift", "bracket_left"), ("i",)]:
        key(vm, *chord)
    expect_screen(vm, directory, "composition")  # êï
    for _ in range(3):
        key(vm, "backspace")
    expect_screen(vm, directory, "demarrage")    # Deux caractères, prompt intact.
    for chord in [("q",), ("shift", "q"), ("w",), ("2",),
                  ("shift_r", "2"), ("alt_r", "e"), ("x",)]:
        key(vm, *chord)
    expect_screen(vm, directory, "frappe")       # aAzé2€x
    for code in ["backspace", "ret", "backspace", "q"]:
        key(vm, code)
    expect_screen(vm, directory, "saisie")       # aAzé2€ validé, puis nova> a

    # La ligne contient déjà « a ». La 129e frappe doit être ignorée.
    for _ in range(127):
        key(vm, "x")
    key(vm, "w")
    for _ in range(127):
        key(vm, "backspace")
    expect_screen(vm, directory, "saisie")

    # Forcer le défilement. Le dernier prompt doit être identique au premier
    # et la bannière doit garder exactement les mêmes pixels.
    for _ in range(16):
        key(vm, "ret")
    ppm = directory / "defilement.ppm"
    rows = initial.split(b"\n", 3)[3]
    stride = 800 * 3
    first = 330 * stride
    height = 20 * stride
    prompt = rows[first:first + height]
    # Les seize premières rangées de pixels portent les glyphes. Les quatre
    # dernières d’une ligne validée sont uniformes, le curseur ayant disparu.
    empty_line = prompt[:16 * stride] + rows[:4 * stride]
    expected = rows[:first] + empty_line * 12 + prompt + rows[first + 13 * height:]
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        qmp(vm, "screendump", filename=str(ppm))
        if ppm.read_bytes().split(b"\n", 3)[3] == expected:
            return
        time.sleep(0.03)
    raise RuntimeError("Défilement incorrect de la zone de saisie.")


def check_x86(vm, directory, log):
    wait_for_serial(vm, log, "Affichage prêt.\r\nClavier PS/2 prêt (AZERTY).")
    test_keyboard(vm, directory)
    wait_for_halt(vm, True)


def check_without_vga(vm, directory, log):
    wait_for_serial(vm, log, "Erreur : aucun framebuffer transmis par Limine.")
    wait_for_halt(vm, False)


def check_arm(vm, directory, log):
    expect_screen(vm, directory, "demarrage-aarch64", timeout=45)
    wait_for_wfi(vm)


def wait_for_wfi(vm):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        code = qmp(vm, "human-monitor-command", **{"command-line": "x/2i $pc-4"})
        if re.search(r"\bwfi\b", code):
            return
        time.sleep(0.03)
    raise RuntimeError("Le processeur ARM64 n’a pas atteint WFI.")


def boot(name, check, **settings):
    with tempfile.TemporaryDirectory(dir=BUILD) as work:
        directory = Path(work)
        log = directory / "serie.log"
        args = ["sh", "scripts/qemu.sh", "-display", "none", "-monitor", "none",
                "-serial", f"file:{log}", "-qmp", "stdio"]
        with subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              bufsize=0, env={**os.environ, **settings}) as vm:
            try:
                qmp(vm, "qmp_capabilities")
                check(vm, directory, log)
                qmp(vm, "quit")
                if vm.wait(timeout=5):
                    raise RuntimeError("Arrêt anormal de QEMU.")
            finally:
                if vm.poll() is None:
                    vm.kill()
                    vm.wait()
                if log.exists():
                    log.replace(BUILD / f"serie-{name}.log")
        print(f"OK — {name}", flush=True)


def check_fault(report, arch, access, cause):
    probe = re.search(r"SONDE adresse=(0x[0-9a-f]+) acces=(\w+)", report)
    fault = re.search(r"(#PF|ABORT) adresse=(0x[0-9a-f]+) pc=(0x[0-9a-f]+) ([^\r\n]+)\n", report)
    if not probe or not fault:
        return False
    tag, address, pc, detail = fault.groups()
    valid = (probe.groups() == (address, access)
             and f"acces={access} cause={cause}" in detail
             and (pc == address if access == "execution" else int(pc, 16) >= 0xffffffff80000000))
    if arch == "x86_64":
        code = int(cause == "protection") | {"lecture": 0, "ecriture": 2, "execution": 16}[access]
        valid &= tag == "#PF" and f"code={code:#x} " in detail and "utilisateur=false" in detail
    else:
        syndrome = re.search(r"esr=(0x[0-9a-f]+)", detail)
        esr = int(syndrome[1], 16) if syndrome else 0
        valid &= (tag == "ABORT" and esr >> 26 == (0x21 if access == "execution" else 0x25)
                  and (esr & 63) in (range(4, 8) if cause == "absente" else range(12, 16))
                  and bool(esr & 64) == (access == "ecriture"))
    if not valid:
        raise RuntimeError(f"Syndrome ou adresse inattendus : {fault[0]}")
    return True


def memory_check(arch, access, cause):
    def verify(vm, directory, log):
        deadline = time.monotonic() + 45
        report = ""
        while time.monotonic() < deadline and vm.poll() is None:
            report = log.read_text(errors="replace") if log.exists() else ""
            if "Panique du noyau" in report:
                raise RuntimeError(report[-550:])
            if ("MMU : sections et alias verifies" in report and
                    "TEST cadres recyclage zero, map/unmap, erreurs et restitution OK" in report):
                if access is None:
                    passed = ("TEST racines independantes, RX=42 et epuisement transactionnel OK" in report
                              and "Affichage prêt." in report)
                else:
                    passed = check_fault(report, arch, access, cause)
                if passed:
                    if arch == "x86_64":
                        wait_for_halt(vm, access is None)
                    else:
                        wait_for_wfi(vm)
                    return
            time.sleep(0.05)
        raise RuntimeError(f"Diagnostic mémoire absent ({access}, {cause})\n{report[-550:]}")
    return verify


def memory_scenarios(arch):
    # Chaque VM démarre le même ELF. Seule la ligne de commande Limine change.
    cases = [
        ("mem-test", None, None),
        ("mem-ro", "ecriture", "protection"),
        ("mem-alias", "ecriture", "protection"),
        ("mem-page-ro", "ecriture", "protection"),
        ("mem-nx", "execution", "protection"),
        ("mem-absent", "lecture", "absente"),
    ]
    for name, access, cause in cases:
        scenario_vm(arch, name, memory_check(arch, access, cause))


def scenario_vm(arch, name, check, suffix=""):
    with tempfile.TemporaryDirectory(dir=BUILD) as work:
        iso = str(Path(work) / "sonde.iso")
        subprocess.run(["sh", "scripts/iso.sh", name, iso], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        boot(f"{arch}-{name}{suffix}", check, NOVA_ISO=iso)


def heap_check(arch, name):
    markers = {
        "heap-test": ("TAS fragmentation, fusion et reutilisation OK",
                      "TAS alignements 1..2M, realloc et realignement OK",
                      "TAS pointeur interieur, disposition et double liberation rejetes",
                      "TAS 1500 operations mixtes OK", "TAS Box Vec String OK", "Affichage prêt."),
        "heap-oom": ("TAS penurie physique, rollback, realloc conserve et reprise OK",
                     "Affichage prêt."),
        "heap-reentrant": ("Erreur fatale du tas : Reentrant",),
        "heap-metadata": ("Erreur fatale du tas : Metadata",),
        "heap-corrupt": ("Erreur fatale du tas : Overrun",),
        "heap-infallible": ("SONDE allocation infaillible", "Panique du noyau",
                            "memory allocation of 268435457 bytes failed"),
    }

    def verify(vm, directory, log):
        deadline = time.monotonic() + (120 if name == "heap-oom" else 45)
        report = ""
        while vm.poll() is None and time.monotonic() < deadline:
            if log.exists():
                report = log.read_text(errors="replace")
            if "Panique du noyau" in report and name != "heap-infallible":
                raise RuntimeError(report[-700:])
            if name == "heap-guard":
                passed = check_fault(report, arch, "lecture", "absente")
            else:
                passed = all(marker in report for marker in markers[name])
            if passed:
                if arch == "x86_64":
                    wait_for_halt(vm, name in ("heap-test", "heap-oom"))
                else:
                    wait_for_wfi(vm)
                return
            time.sleep(0.05)
        raise RuntimeError(f"Diagnostic absent pour {name} : {report[-700:]}")
    return verify


def scheduler_check(arch, name):
    def verify(vm, directory, log):
        start = time.monotonic()
        deadline = start + (160 if name == "sched-stress" else 45)
        report = ""
        while vm.poll() is None and time.monotonic() < deadline:
            report = log.read_text(errors="replace") if log.exists() else ""
            if "Panique du noyau" in report:
                raise RuntimeError(report[-1200:])
            if name == "sched-guard":
                passed = check_fault(report, arch, "ecriture", "absente")
            elif name == "sched-fpu":
                passed = ("FPU/SIMD interdit : " + ("#NM" if arch == "x86_64" else "EC=7")) in report
            else:
                proof = re.search(r"PREEMPT ticks=(\d+) switches=(\d+) A=(\d+) B=(\d+) yields=0", report)
                passed = (proof is not None and int(proof[1]) >= 100 and int(proof[2]) >= 80
                          and min(int(proof[3]), int(proof[4])) > 5
                          and "ORDONNANCEUR validation OK" in report and "Affichage prêt." in report)
                if name == "sched-test":
                    checks = re.search(r"THREADS compteur=40 attentes=(\d+) yields=(\d+) reveils=(\d+) collectes=(\d+) idle=(\d+)", report)
                    passed = (passed and checks is not None and int(checks[1]) > 0
                              and int(checks[2]) >= 40 and int(checks[3]) >= 40
                              and int(checks[4]) >= 4 and int(checks[5]) > 0
                              and "PILES 48 creations et restitutions OK" in report
                              and "IRQ timer pendante masquee puis livree OK" in report
                              and "BUDDY IRQ masquee sous verrou puis livree OK" in report
                              and "MUTEX transfert FIFO sans depassement OK" in report
                              and "TCB saturation, generations et affinite CPU0 OK" in report)
                else:
                    stats = re.search(r"STRESS ticks=(\d+) switches=(\d+) A=(\d+) B=(\d+) cycles=(\d+)", report)
                    passed = (passed and stats is not None and int(stats[1]) >= 15000
                              and int(stats[2]) >= 14000 and min(int(stats[3]), int(stats[4])) >= 3000
                              and int(stats[5]) >= 50)
            if passed:
                if arch == "x86_64":
                    wait_for_halt(vm, name in ("sched-test", "sched-stress"))
                else:
                    wait_for_wfi(vm)
                if name == "sched-stress":
                    print(f"{arch} : {stats[0]} ({time.monotonic()-start:.1f} s avec amorçage)")
                return
            time.sleep(0.05)
        raise RuntimeError(f"Validation absente : {arch} {name}\n{report[-1200:]}")
    return verify


def endurance(arch, seconds):
    # Chaque bilan est émis après la collecte de tous les travailleurs. On garde
    # la même VM et le même espace mémoire entre les périodes.
    result = {}
    def verify(vm, directory, log):
        deadline = time.monotonic() + 120
        offset, pending, start, baseline, previous = 0, "", None, None, {}
        growing = ("ticks", "preemptions", "collectes", "attentes", "mutex", "A", "B", "allocations", "audits")
        while vm.poll() is None and time.monotonic() < deadline:
            if log.exists():
                with log.open("rb") as stream:
                    stream.seek(offset)
                    pending += stream.read().decode("ascii", errors="replace")
                    offset = stream.tell()
            lines = pending.split("\n")
            pending = lines.pop()
            for line in lines:
                if any(bad in line for bad in ("Panique", "Erreur fatale", "#PF", "ABORT", "FPU/SIMD interdit")):
                    raise RuntimeError(line)
                if not line.startswith("ENDURANCE "):
                    continue
                values = {k: int(v) for k, v in re.findall(r"([A-Za-z]+)=(\d+)", line)}
                resources = {k: values[k] for k in ("blocs", "occupes", "engages", "libres")}
                now = time.monotonic()
                if line.startswith("ENDURANCE debut "):
                    if start is not None:
                        raise RuntimeError("Redémarrage pendant la campagne")
                    start, baseline = now, resources
                elif line.startswith("ENDURANCE bilan "):
                    if (resources != baseline or values["periode"] != previous.get("periode", 0) + 1
                            or any(values[k] <= previous.get(k, 0) for k in growing)
                            or values["penuries"] != 1 + (values["periode"] - 1) // 64
                            or not 0 < values["minimum"] < baseline["libres"]):
                        raise RuntimeError(f"Bilan incohérent : {line}")
                    elapsed = now - start
                    previous = values
                    if elapsed >= seconds:
                        result.update(architecture=arch, secondes_demandees=seconds,
                                      secondes_reelles=round(elapsed, 3), avant=baseline, bilan=values)
                        return
                deadline = now + 120
            time.sleep(0.1)
        raise RuntimeError(f"Endurance interrompue ou bilan absent depuis 120 s : {previous}")
    scenario_vm(arch, "sched-endurance", verify, f"-{seconds}s")
    path = BUILD / f"endurance-{arch}-{seconds}s.json"
    path.write_text(json.dumps(result, indent=2) + "\n")
    print(path, result, flush=True)


def suite():
    if os.environ.get("NOVA_ARCH", "x86_64") == "aarch64":
        boot("aarch64", check_arm)
    else:
        boot("std", check_x86, NOVA_VGA="std")
        boot("none", check_without_vga, NOVA_VGA="none")
    arch = os.environ.get("NOVA_ARCH", "x86_64")
    memory_scenarios(arch)
    for case in ("heap-test", "heap-oom", "heap-corrupt", "heap-guard", "heap-infallible", "heap-reentrant", "heap-metadata"):
        scenario_vm(arch, case, heap_check(arch, case))
    for case in ("sched-test", "sched-guard", "sched-fpu", "sched-stress"):
        scenario_vm(arch, case, scheduler_check(arch, case))

    endurance(arch, 15)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--endurance", type=int, metavar="SECONDES")
    args = parser.parse_args()
    if args.endurance is None:
        suite()
    elif args.endurance > 0:
        endurance(os.environ.get("NOVA_ARCH", "x86_64"), args.endurance)
    else:
        parser.error("La durée doit être strictement positive.")
