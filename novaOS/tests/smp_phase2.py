"""Exerce l'arrêt terminal et les calculs utilisateur dans des VM indépendantes."""
import argparse
import os
import re
import time

from demarrage import process_test, scenario_vm


def run(arch, name, cpus):
    full = (1 << cpus) - 1
    absent = 1 << (cpus - 1) if name == "smp-ap-absent" else 0
    missing = 2 if name == "smp-cpu-muet" else 0
    leader = int(name == "smp-panique" and cpus > 1)
    reason = {"smp-panique": "panique", "smp-ap-absent": "amorcage"}.get(name, "normal")
    expected = (f"SMP arret motif={reason} coordinateur={leader} attendus={full:#x} "
                f"en_ligne={full ^ absent:#x} ack={full ^ absent ^ missing:#x} "
                f"absents={absent:#x} muets={missing:#x}")

    def verify(vm, directory, log):
        end = time.monotonic() + 30
        text = ""
        while time.monotonic() < end and vm.poll() is None:
            text = log.read_text(errors="replace") if log.exists() else ""
            if expected in text:
                parked = [(int(cpu), int(sp, 16)) for cpu, sp in
                          re.findall(r"SMP diagnostic cpu=(\d+) sp=(0x[0-9a-f]+)", text)]
                assert sorted(cpu for cpu, _ in parked) == [
                    i for i in range(cpus) if (full ^ absent ^ missing) & (1 << i)]
                assert all(sp for _, sp in parked) and len(set(sp for _, sp in parked)) == len(parked)
                if name == "smp-fp":
                    cycles = re.findall(r"SMP FPU cycle=(\d+) clients=(\d+) migrations=(\d+) OK", text)
                    assert cycles == [(str(i), str(2 * cpus), str(2 * cpus if cpus > 1 else 0)) for i in range(2)]
                return
            if "SMP arret motif=" in text and text.endswith("\n"):
                break
            time.sleep(.04)
        raise RuntimeError(f"{expected}\n{text[-1800:]}")

    profile = os.environ.get("NOVA_X86_CPU", "") if arch == "x86_64" else "gic" + os.environ.get("NOVA_GIC", "2")
    suffix = re.sub(r"[^a-zA-Z0-9_.-]", "_", profile)
    scenario_vm(arch, name, verify, f"-{cpus}cpu-{suffix}", NOVA_VCPUS=str(cpus))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", choices=["arret", "fp"])
    a = parser.parse_args()
    for n in (1, 2, 4):
        names = ["smp-fp"] if a.profile == "fp" else ["smp-arret", "smp-panique"]
        if n > 1 and a.profile == "arret":
            names += ["smp-ap-absent", "smp-cpu-muet"]
        for name in names:
            run(os.environ["NOVA_ARCH"], name, n)

    if a.profile == "fp":
        process_test(os.environ["NOVA_ARCH"], user_fp=True)
