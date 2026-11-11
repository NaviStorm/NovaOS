"""Démarrage du profil ELF et vérification de la fin des sondes en espace utilisateur."""
import os
import time
from demarrage import scenario_vm

def verify(vm, directory, log):
    deadline = time.monotonic() + 60
    while vm.poll() is None and time.monotonic() < deadline:
        report = log.read_text(errors="replace") if log.exists() else ""
        if "Panique du noyau" in report or "Erreur fatale" in report:
            raise RuntimeError(report[-1500:])
        for line in report.rpartition('\n')[0].splitlines():
            if line.startswith("ELF restitution OK") and " hz=" in line:
                print(line, flush=True)
                return
        time.sleep(0.05)
    raise RuntimeError(f"Bilan ELF absent dans {log}")

if __name__ == "__main__":
    scenario_vm(os.environ["NOVA_ARCH"], "elf-test", verify)
