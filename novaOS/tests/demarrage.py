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


ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / "build"
BANNER = b"NovaOS v0.16.3\r\n"
# Captures PPM intégrales vérifiées visuellement en 800 × 600.
SCREENS = {
    "demarrage-aarch64": "7a05d5c3c920faf2084437a3105eb059479065f28c0da37848a53d7ec80b4ba3",
    "demarrage": "0dc1e4a7d37dfabeac909cf6447c88c7c3a740904ded40231a3612176397af83",
    "composition": "3698d6ab00f44a5145d0e0b1c30a615ec5859c6626c340318ebbd8116dca3e9c",
    "frappe": "7af8d4a85b295227cabff3d91f66b00752cc4ed7334a55dbae0d02df8030777a",
    "saisie": "6f46585fe084aaf27d2ac7203c3bbd37e2b423da88238ad88b324331c178edf0",
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


def wait_demo(vm, log, level, faulty=False):
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline and vm.poll() is None:
        report = log.read_text(errors="replace") if log.exists() else ""
        if "Panique du noyau" in report:
            raise RuntimeError(report[-800:])
        first = "faute=" if faulty else "sortie=41"
        if ("PROC demonstration terminee" in report
                and re.search(rf"PROC pid=1 niveau={level} pc=0x[0-9a-f]+ {first}", report)
                and re.search(rf"PROC pid=2 niveau={level} pc=0x[0-9a-f]+ sortie=82\b", report)):
            return
        time.sleep(0.05)
    raise RuntimeError(f"Démonstration inachevée en {level}")


def wait_storage(vm, log, headless=False):
    deadline = time.monotonic() + 10
    while vm.poll() is None and time.monotonic() < deadline:
        report = log.read_text(errors="replace") if log.exists() else ""
        marker = "STOCKAGE arret service=" if headless else "RAM phase=stockage_boot"
        if (marker in report and "STOCKAGE session=1" in report
                and "INFO volumes=0 FIN confirmee" in report):
            return
        time.sleep(0.05)
    raise RuntimeError("Échange de démarrage avec le service de stockage inachevé.")


def check_x86(vm, directory, log):
    wait_for_serial(vm, log, "Affichage prêt.\r\nClavier PS/2 prêt (AZERTY).")
    wait_demo(vm, log, "Ring3")
    wait_storage(vm, log)
    test_keyboard(vm, directory)
    wait_for_halt(vm, True)


def check_without_vga(vm, directory, log):
    wait_for_serial(vm, log, "Erreur : aucun framebuffer transmis par Limine.")
    wait_storage(vm, log, headless=True)
    wait_for_halt(vm, False)


def check_arm(vm, directory, log):
    wait_demo(vm, log, "EL0")
    wait_storage(vm, log)
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
        args = ["sh", ROOT / "scripts/qemu.sh", "-display", "none", "-monitor", "none",
                "-serial", f"file:{log}", "-qmp", "stdio"]
        with subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              bufsize=0, env={**os.environ, **settings}) as vm:
            try:
                qmp(vm, "qmp_capabilities")
                result = check(vm, directory, log)
                qmp(vm, "quit")
                if vm.wait(timeout=5):
                    raise RuntimeError("Arrêt anormal de QEMU.")
            finally:
                if vm.poll() is None:
                    vm.kill()
                    vm.wait()
                if log.exists():
                    log.replace(BUILD / f"serie-{name}.log")
        print(f"{result or 'OK'} — {name}", flush=True)
        return result


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


def scenario_vm(arch, name, check, suffix="", **settings):
    BUILD.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir=BUILD) as work:
        iso = str(Path(work) / "sonde.iso")
        subprocess.run(["sh", ROOT / "scripts/iso.sh", name, iso], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        return boot(f"{arch}-{name}{suffix}", check, NOVA_ISO=iso, **settings)


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


def process_test(arch):
    def verify(vm, directory, log):
        deadline = time.monotonic() + 45
        report = ""
        while vm.poll() is None and time.monotonic() < deadline:
            report = log.read_text(errors="replace") if log.exists() else ""
            if any(marker in report for marker in ("Panique du noyau", "Exception noyau", "Erreur fatale")):
                raise RuntimeError(report[-1500:])
            if "Affichage prêt." in report:
                break
            time.sleep(0.05)
        level = "Ring3" if arch == "x86_64" else "EL0"
        events = re.findall(rf"PROC pid=(\d+) niveau={level} pc=(0x[0-9a-f]+) ([^\r\n]+)", report)
        resources = re.search(r"PROC ressources avant=(\d+) apres=(\d+) blocs=(\d+) restitution OK", report)
        runs = re.search(r"PROC concurrence preemptions=(\d+),(\d+) progressions=(\d+),(\d+) registres OK", report)
        if (len(events) != 78 or len({pid for pid, _, _ in events}) != len(events)
                or not resources or resources[1] != resources[2] or not runs
                or min(map(int, runs.groups()[:2])) < 8 or min(map(int, runs.groups()[2:])) <= 0
                or "PROC penuries 0,2,5,9,20 cadres, rollback OK" not in report
                or "PROC orphelin collecte sous charge OK" not in report
                or "PROC 64 creations et collectes OK" not in report or "Affichage prêt." not in report):
            raise RuntimeError("Validation des processus incomplète\n" + report[-1800:])
        faults = [e for e in events if "faute=" in e[2]]
        expected = [
            (0x600000, "lecture", "absente", 4), (0xffffffff80000000, "lecture", "protection", 5),
            (0x600000, "ecriture", "protection", 7),
            (0x400000, "ecriture", "protection", 7), (0x401000, "execution", "protection", 21),
            (0x7ffff8, "ecriture", "absente", 6), (0x801000, "ecriture", "absente", 6),
            (0, "instruction", "invalide", 0),
            ((1 << 47), "retour", "pile", 0) if arch == "x86_64" else ((1 << 47), "ecriture", "absente", 6),
            (0, "instruction", "privilege", 0), (0, "instruction", "fpu", 0),
        ]
        assert len(faults) == len(expected)
        for (_, pc, detail), (addr, access, cause, code) in zip(faults, expected):
            assert f"adresse={addr:#x} acces={access} cause={cause} " in detail, detail
            syndrome = int(detail.split("syndrome=")[-1], 16)
            if arch == "x86_64":
                assert syndrome == code, detail
            elif access != "instruction":
                assert syndrome >> 26 == (0x20 if access == "execution" else 0x24), detail
                assert (syndrome & 63) in (range(4,8) if cause == "absente" else range(12,16)), detail
                assert bool(syndrome & 64) == (access == "ecriture"), detail
            else:
                assert syndrome >> 26 in {"privilege": (0, 0x18), "fpu": (7,), "invalide": (0x15,)}[cause], detail
            assert (int(pc,16) == addr if access == "execution" else 0x400000 <= int(pc,16) < 0x401000)
        # Les deux survivants sortent volontairement après la dernière faute.
        exits = [(int(pid), detail) for pid, _, detail in events if "sortie=" in detail]
        assert exits[0][1] == "sortie=23"
        assert sorted(exits[1:3]) == [(1,"sortie=41"), (2,"sortie=82")]
        assert all(detail == "sortie=17" for _, detail in exits[3:])
        assert report.index("cause=fpu") < report.index("sortie=41")
        if arch == "x86_64":
            wait_for_halt(vm, True)
        else:
            wait_for_wfi(vm)
    scenario_vm(arch, "proc-test", verify)


def boot_fault(arch):
    def verify(vm, directory, log):
        wait_demo(vm, log, "Ring3" if arch == "x86_64" else "EL0", faulty=True)
        if arch == "x86_64":
            test_keyboard(vm, directory)
        else:
            expect_screen(vm, directory, "demarrage-aarch64")
            wait_for_wfi(vm)
    scenario_vm(arch, "proc-boot-fault", verify)


def abi_scenario(arch, ram, allow_firmware_limit=False, arm_pan=False):
    measured = {}
    def verify(vm, directory, log):
        configured = qmp(vm, "query-memory-size-summary")["base-memory"]
        if configured != ram * 1024**2:
            raise RuntimeError(f"RAM QEMU inattendue : {configured}")
        deadline = time.monotonic() + 55
        report = ""
        while vm.poll() is None and time.monotonic() < deadline:
            report = log.read_text(errors="replace") if log.exists() else ""
            clean = re.sub(r"\x1b\[[0-9;=?]*[a-zA-Z]", "", report)
            if allow_firmware_limit and ram == 64 and "NovaOS v" not in clean:
                reason = ("NewSize >= 0x08000000" if arch == "aarch64" else
                          "High memory allocator: Out of memory")
                if reason in clean:
                    measured.update(etat="avant_noyau", motif=reason)
                    return "NON EXÉCUTÉ (amorçage)"
            if any(bad in report for bad in ("Panique du noyau", "Exception noyau", "Erreur fatale")):
                raise RuntimeError(report[-1800:])
            # La sortie série est progressive : attendre la ligne entière avant de
            # vérifier le total des cadres (resident et retenu se trouvent en fin de ligne).
            if re.search(r"RAM phase=repos_ui [^\n]*\n", report) and "ABI valide" in report:
                break
            time.sleep(0.05)
        phases = {name: {k: int(v) for k,v in re.findall(r"([a-z_]+)=(\d+)", fields)}
                  for name,fields in re.findall(r"RAM phase=([a-z_]+) ([^\r\n]+)", report)}
        assert set(phases) == {"init","repos","reprise","actifs","detruits","repos_ui"}, report[-1200:]
        for v in phases.values():
            assert v["resident"] == sum(v[k] for k in ("image","tables","kpile","upile","heap","buddy","ipc","utilisateur","dma"))
            assert v["retenu"] == v["resident"] + v["chargeur"] + v["extra_elf"] + v["ram_video"]
            assert v["retenu"] + v["libres"] <= ram * 256
        assert "ABI copie sur deux pages et retrait atomique OK" in report
        assert phases["actifs"]["meta_actifs"] > phases["repos"]["meta_actifs"] == 0
        assert phases["repos_ui"]["heap"] <= phases["repos_ui"]["heap_va"]
        assert phases["init"]["chargeur"] == phases["repos"]["restitue"] > 0
        assert phases["repos"] == phases["detruits"] == phases["reprise"]
        assert phases["actifs"]["processus"] == 2 and phases["actifs"]["upile"] == 2
        assert phases["repos_ui"]["processus"] == phases["repos_ui"]["chargeur"] == 0
        proof = re.search(r"ABI valide series=(\d+) preemptions=(\d+) attentes=(\d+) revocation=OK", report)
        assert proof and int(proof[1]) >= 4096 and int(proof[2]) > 0 and int(proof[3]) >= 2
        assert len(re.findall(r"^DEBUG pid=\d+ NovaABI$",report,re.M)) == 16
        assert len(re.findall(r"faute=ABI .*acces=retour cause=contexte",report)) == 2
        assert "pc=0x401000 faute=ABI" in report
        if arch == "x86_64":
            paths = re.search(r"ABI SYSRET=(\d+) IRETQ=(\d+) SMAP=(\d)",report)
            assert paths and min(int(paths[1]),int(paths[2])) > 0 and paths[3] == "1"
            measured.update(sysret=int(paths[1]), iretq=int(paths[2]), smap=True)
            wait_for_halt(vm, True)
        else:
            feature = re.search(r"ABI ERET PAN=(0|1)", report)
            assert feature and (not arm_pan or feature[1] == "1")
            measured["pan"] = feature[1] == "1"
            wait_for_wfi(vm)
        measured.update(etat="valide", phases=phases, objectif_8_mio=phases["repos_ui"]["retenu"] <= 2048, series=int(proof[1]),
                        preemptions=int(proof[2]), attentes=int(proof[3]))

    label = f"{ram}-pan" if arm_pan else str(ram)
    extra = {"NOVA_ARM_CPU": "cortex-a76"} if arm_pan else {}
    scenario_vm(arch, "abi-test", verify, f"-{label}", NOVA_RAM=str(ram), **extra)
    (BUILD / f"abi-{arch}-{label}.json").write_text(json.dumps(measured, indent=2) + "\n")


def storage_test(arch):
    level = "Ring3" if arch == "x86_64" else "EL0"

    def verify(vm, directory, log):
        deadline = time.monotonic() + 55
        report = ""
        while vm.poll() is None and time.monotonic() < deadline:
            report = log.read_text(errors="replace") if log.exists() else ""
            if any(s in report for s in ("Panique du noyau", "Exception noyau", "Erreur fatale")):
                raise RuntimeError(report[-2400:])
            if "STOCKAGE validation OK" in report:
                break
            time.sleep(0.05)
        assert "STOCKAGE validation OK" in report and "faute=" not in report, report[-2400:]
        events = re.findall(rf"PROC pid=(\d+) niveau={level} pc=0x40[0-9a-f]{{4}} sortie=(\d+)", report)
        exits = dict(events)
        assert len(events) == 8 and exits.pop("3", None) in ("5", "11"), events
        assert exits == {"1": "5", "2": "70", "4": "0", "5": "0", "6": "0", "7": "0", "8": "0"}, events
        assert "STOCKAGE session=2 service=4 client=5 INFO volumes=0 FIN confirmee" in report
        assert "STOCKAGE seconde session service=4 client=6" in report
        assert "STOCKAGE arret service=4 resultats=Exit(0),Exit(0)" in report
        assert "IPC hist type=aller_retour n=513 " in report
        assert "STOCKAGE campagne tours=513 " in report
        assert re.search(r"IPC hist type=commutation n=[1-9]\d* p50=[1-9]", report)
        assert re.search(r"STOCKAGE trafic tours=513 ns=[1-9]", report)
        assert re.search(r"STOCKAGE cadres allocations=[1-9]\d* pic=[1-9]", report)
        assert "STOCKAGE publication annulee budget=" in report
        phases = {phase: dict(re.findall(r"(\w+)=(\d+)", values)) for phase, values in
                  re.findall(r"RAM phase=stockage_(\w+) ([^\n]+)", report)}
        assert phases["repos"] == phases["recupere"] == phases["detruit"]
        assert phases["resident"]["processus"] == "1" and phases["resident"]["upile"] == "1"
        assert phases["publie"]["processus"] == "2" and phases["publie"]["upile"] == "2"

    scenario_vm(arch, "storage-test", verify)


def suite():
    if os.environ.get("NOVA_ARCH", "x86_64") == "aarch64":
        boot("aarch64", check_arm)
    else:
        boot("std", check_x86, NOVA_VGA="std")
        boot("none", check_without_vga, NOVA_VGA="none")
    arch = os.environ.get("NOVA_ARCH", "x86_64")
    abi_scenario(arch, 256)
    if arch == "aarch64":
        abi_scenario(arch, 256, arm_pan=True)
    storage_test(arch)
    process_test(arch)
    boot_fault(arch)
    memory_scenarios(arch)
    for case in ("heap-test", "heap-oom", "heap-corrupt", "heap-guard", "heap-infallible", "heap-reentrant", "heap-metadata"):
        scenario_vm(arch, case, heap_check(arch, case))
    for case in ("sched-test", "sched-guard", "sched-fpu", "sched-stress"):
        scenario_vm(arch, case, scheduler_check(arch, case))

    endurance(arch, 15)
    from ipc import campaign
    campaign(arch)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--endurance", type=int, metavar="SECONDES")
    parser.add_argument("--ram-test", action="store_true")
    parser.add_argument("--storage-test", action="store_true")
    args = parser.parse_args()
    if args.storage_test:
        storage_test(os.environ.get("NOVA_ARCH", "x86_64"))
    elif args.ram_test:
        from ipc import campaign
        arch = os.environ.get("NOVA_ARCH", "x86_64")
        for ram in (64, 128, 256):
            abi_scenario(arch, ram, allow_firmware_limit=True)
            if ram >= 128:
                campaign(arch, ram=ram)
    elif args.endurance is None:
        suite()
    elif args.endurance > 0:
        endurance(os.environ.get("NOVA_ARCH", "x86_64"), args.endurance)
    else:
        parser.error("La durée doit être strictement positive.")
