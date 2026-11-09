"""Démarrages UEFI et saisie PS/2 sur le même ISO que make run."""

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
BANNER = b"NovaOS v0.2.0\r\n"
# Captures PPM intégrales vérifiées visuellement en 800 × 600.
SCREENS = {
    "demarrage": "f7895d6ba3fdaa1ed28bde65e6cb75e4a417b8dd24f332e22281644c78792a71",
    "composition": "bb1adcbafd0ebc095229851235dc6332e5c17491afee2b57568142bce96d9b2e",
    "frappe": "659a74e4ce6c8734be383794cb258c510cf81ba1df688f6e9604b269ba8a72da",
    "saisie": "6172483604bfcf28e7c7d5f3f5abc0daf9dc42f18a4a6067ed624a74713114d0",
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


def expect_screen(vm, directory, name):
    ppm = directory / "courant.ppm"
    deadline = time.monotonic() + 5
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


def boot(vga, expected):
    with tempfile.TemporaryDirectory(dir=BUILD) as work:
        directory = Path(work)
        log = directory / "serie.log"
        args = ["sh", "scripts/qemu.sh", "-display", "none", "-monitor", "none",
                "-serial", f"file:{log}", "-qmp", "stdio"]
        with subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              bufsize=0, env={**os.environ, "NOVA_VGA": vga}) as vm:
            try:
                wait_for_serial(vm, log, expected)
                qmp(vm, "qmp_capabilities")
                if vga == "std":
                    test_keyboard(vm, directory)
                wait_for_halt(vm, vga == "std")
                qmp(vm, "quit")
                if vm.wait(timeout=5):
                    raise RuntimeError("Arrêt anormal de QEMU.")
            finally:
                if vm.poll() is None:
                    vm.kill()
                    vm.wait()
                if log.exists():
                    log.replace(BUILD / f"serie-{vga}.log")
        print(f"OK — VGA {vga}, {expected} Processeur en veille.")


if __name__ == "__main__":
    boot("std", "Affichage prêt.\r\nClavier PS/2 prêt (AZERTY).")
    boot("none", "Erreur : aucun framebuffer transmis par Limine.")
