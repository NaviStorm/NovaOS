"""Démarrages UEFI de bout en bout, sur le même ISO que make run."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


BUILD = Path("build")
BANNER = b"NovaOS v0.1.0\r\n"
# Capture PPM intégrale de la bannière v0.1.0 vérifiée visuellement en 800 × 600.
REFERENCE = "b323d35cfcf296a4c5008e864513129889605fcd6b67054253b3d356efd55f87"


def wait_for_serial(vm, log, expected):
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline and vm.poll() is None:
        if log.exists() and BANNER + expected.encode() + b"\r\n" in log.read_bytes():
            return
        time.sleep(0.05)
    raise RuntimeError(f"Diagnostic série attendu : {expected}")


def inspect_vm(vm, directory, video):
    commands = [
        {"execute": "qmp_capabilities"},
        {"execute": "human-monitor-command", "arguments": {"command-line": "info registers"}},
    ]
    if video:
        for extension in ("ppm", "png"):
            filename = directory / f"demarrage.{extension}"
            commands.append({"execute": "screendump", "arguments": {
                "filename": str(filename), "format": extension,
            }})
    commands.append({"execute": "quit"})
    for index, command in enumerate(commands):
        command["id"] = index
    requests = "".join(json.dumps(command) + "\n" for command in commands)
    # Garder stdin ouvert jusqu’à ce que QEMU traite lui-même la commande quit.
    vm.stdin.write(requests)
    vm.stdin.flush()
    vm.wait(timeout=10)
    replies = [json.loads(line) for line in vm.stdout]
    results = {reply["id"]: reply for reply in replies if "id" in reply}
    for command in commands:
        reply = results.get(command["id"], {})
        if "return" not in reply:
            raise RuntimeError(f"Échec de {command['execute']} : {reply}")
    if vm.returncode or "HLT=1" not in results[1]["return"]:
        raise RuntimeError("Le processeur n’a pas atteint son état de repos.")


def boot(vga, expected):
    with tempfile.TemporaryDirectory(dir=BUILD) as work:
        directory = Path(work)
        log = directory / "serie.log"
        args = ["sh", "scripts/qemu.sh", "-display", "none", "-monitor", "none",
                "-serial", f"file:{log}", "-qmp", "stdio"]
        with subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              text=True, env={**os.environ, "NOVA_VGA": vga}) as vm:
            try:
                wait_for_serial(vm, log, expected)
                inspect_vm(vm, directory, vga == "std")
            finally:
                if vm.poll() is None:
                    vm.kill()
                    vm.wait()
                if log.exists():
                    log.replace(BUILD / f"serie-{vga}.log")
        if vga == "std":
            (directory / "demarrage.png").replace(BUILD / "demarrage.png")
            frame = (directory / "demarrage.ppm").read_bytes()
            if hashlib.sha256(frame).hexdigest() != REFERENCE:
                raise RuntimeError("Pixels inattendus. Voir build/demarrage.png.")
        print(f"OK — VGA {vga}, {expected} Processeur en veille.")


if __name__ == "__main__":
    boot("std", "Affichage prêt.")
    boot("none", "Erreur : aucun framebuffer transmis par Limine.")
