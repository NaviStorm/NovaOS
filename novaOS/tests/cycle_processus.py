"""Collecte des familles de processus dans une VM neuve."""
import os
import re
import time

from demarrage import scenario_vm


def verifier(vm, directory, log):
    limite = time.monotonic() + 90
    while vm.poll() is None and time.monotonic() < limite:
        serie = log.read_text(errors="replace") if log.exists() else ""
        if "Panique du noyau" in serie or "Exception noyau" in serie:
            raise RuntimeError(serie[-1400:])
        if re.search(r"PROC familles=64 processus=128 cadres=[1-9]\d* restitution OK", serie):
            evenements = re.findall(r"PROC pid=(\d+) niveau=\w+ pc=\w+ (sortie=27|faute=\S+)", serie)
            assert len(evenements) == len({pid for pid, _ in evenements}) == 128
            assert sum(action == "sortie=27" for _, action in evenements) == 96
            return
        time.sleep(0.05)
    raise RuntimeError("Campagne de collecte inachevée")


if __name__ == "__main__":
    scenario_vm(os.environ["NOVA_ARCH"], "proc-lifecycle", verifier)
