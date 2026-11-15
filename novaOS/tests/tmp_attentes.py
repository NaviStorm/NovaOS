"""Interruption déterministe d’un appel avant et après sa remise au service témoin."""
import hashlib
import json
import os
import re
import time

from demarrage import BUILD, scenario_vm


def check(vm, directory, log):
    expected = b"TMP attentes reveil=OK avant=CANCELLED apres=UNCERTAIN retrait=DENIED tickets=0"
    deadline = time.monotonic() + 100
    raw = b""
    while time.monotonic() < deadline:
        if log.exists():
            raw = log.read_bytes()
            lignes = re.findall(rb"TMP fin-service phase=(\d) appel=(\d+) autre=(\d+) effets=(\d+) ancien=(\d+) ticket=(\d+) nouveau=DENIED file=0\r?\n", raw)
            if expected in raw and len(lignes) == 5:
                precedent = 0
                phases = []
                for donnees, (phase, statut, compteur) in zip(lignes, [(5, 23, 0), (4, 0, 1), (3, 22, 1), (2, 22, 0), (1, 11, 0)], strict=True):
                    p, appel, autre, effets, ancien, ticket = map(int, donnees)
                    assert (p, appel, autre, effets, ancien) == (phase, statut, 11, compteur, precedent)
                    assert ticket > ancien
                    phases.append(dict(phase=p, appel=appel, autre=autre, effets=effets, ancien=ancien, ticket=ticket))
                    precedent = ticket
                return dict(historique=expected.decode(), fin_service=phases)
            if b"Panique du noyau" in raw:
                break
        if vm.poll() is not None:
            break
        time.sleep(0.05)
    raise RuntimeError(raw[-2000:].decode(errors="replace"))


if __name__ == "__main__":
    arch = os.environ["NOVA_ARCH"]
    results = {}
    for n in (1, 2, 4):
        result = scenario_vm(arch, "tmp-attentes", check, f"-{n}cpu", NOVA_VCPUS=str(n))
        serial = BUILD / f"serie-{arch}-tmp-attentes-{n}cpu.log"
        results[n] = dict(result=result, serial=serial.name, sha256=hashlib.sha256(serial.read_bytes()).hexdigest())
    (BUILD / f"tmp-attentes-{arch}.json").write_text(json.dumps(results, indent=2) + "\n")
