"""Qualification de la supervision et de la collecte sur trois profils SMP."""
import hashlib
import json
import os
import re
import time

from demarrage import BUILD, scenario_vm

FAMILY = re.compile(
    r"INSTANCE famille mode=(\d+) id=(\d+) pid=(\d+) cpu_cible=(\d+) retrait=OK"
)
BALANCE = re.compile(r"INSTANCE bilan familles=4 cadres=(\d+) tcb=(\d+) OK")


def check(vm, directory, log):
    deadline = time.monotonic() + 100
    text = ""
    while time.monotonic() < deadline:
        if log.exists():
            text = log.read_bytes().decode(errors="replace")
        result = BALANCE.search(text)
        if result:
            families = [tuple(map(int, row)) for row in FAMILY.findall(text)]
            assert len(families) == 4 and {row[0] for row in families} == {0, 1, 2, 3}
            assert len({row[1] for row in families}) == 4
            assert all(pid < 2**63 < serial for _, serial, pid, _ in families)
            assert "INSTANCE catalogue SPAWN principal et travailleur=OK" in text
            assert "anciennes_generations_refusees=OK" in text
            return {"familles": families, "cadres": int(result[1]), "tcb": int(result[2])}
        if vm.poll() is not None or "Panique du noyau" in text:
            break
        time.sleep(0.05)
    raise RuntimeError(text[-1800:])


if __name__ == "__main__":
    arch = os.environ["NOVA_ARCH"]
    runs = {}
    for n in (1, 2, 4):
        run = scenario_vm(arch, "instances-test", check, f"-{n}cpu", NOVA_VCPUS=str(n))
        journal = BUILD / f"serie-{arch}-instances-test-{n}cpu.log"
        run.update(journal=journal.name, sha256=hashlib.sha256(journal.read_bytes()).hexdigest())
        assert run["tcb"] == n + 1 and all(row[3] == n - 1 for row in run["familles"])
        runs[n] = run
    (BUILD / f"instances-{arch}.json").write_text(json.dumps(runs, indent=2) + "\n")
