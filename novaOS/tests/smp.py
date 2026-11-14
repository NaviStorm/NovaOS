"""Vérifie dans la VM la participation de chaque processeur et son timer local."""
import os
import re
import time
from demarrage import scenario_vm


def run(arch, cpus):
    def check(vm, directory, log):
        text = ""
        start = time.monotonic()
        while time.monotonic() - start < 45 and vm.poll() is None:
            text = log.read_text(errors="replace") if log.exists() else ""
            if f"SMP rendezvous={((1 << cpus) - 1):#x} actifs={cpus} OK" in text:
                rows = [tuple(map(int, m)) for m in re.findall(r"SMP cpu=(\d+) materiel=(\d+) ticks=(\d+)", text)]
                assert sorted(c for c, _, _ in rows) == list(range(cpus))
                assert len({h for _, h, _ in rows}) == cpus and all(t >= 4 for _, _, t in rows)
                actors = re.findall(r"SMP utilisateur pid=(\d+) cpu=(\d+) preemptions=(\d+) progres=(\d+)", text)
                assert sorted(int(c) for _, c, _, _ in actors) == [i for i in range(cpus) for _ in range(2)]
                assert len({p for p, _, _, _ in actors}) == 2 * cpus
                assert all(int(t) > 0 and int(v) > 0 for _, _, t, v in actors)
                assert f"SMP stationnes={cpus-1} en_ligne=1" in text
                if cpus > 1:
                    assert "SMP migration=0->1" in text
                    assert f"SMP traductions distantes masque={((1 << cpus) - 2):#x} OK" in text
                return "OK"
            if any(s in text for s in ("PANIC", "Panique", "Exception noyau", "#PF adresse", "ABORT adresse")):
                break
            time.sleep(0.05)
        raise RuntimeError(text[-2400:])
    gic = "-gic3" if arch == "aarch64" and os.environ.get("NOVA_GIC") == "3" else ""
    model = os.environ.get("NOVA_X86_CPU" if arch == "x86_64" else "NOVA_ARM_CPU")
    suffix = "-" + re.sub(r"[^a-zA-Z0-9_.-]", "_", model) if model else ""
    scenario_vm(arch, "smp-test", check, f"-{cpus}cpu{gic}{suffix}", NOVA_VCPUS=str(cpus))


if __name__ == "__main__":
    for count in (1, 2, 4):
        run(os.environ["NOVA_ARCH"], count)
