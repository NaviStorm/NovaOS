"""Campagnes IPC en CPL3/EL0. Un processus hôte surveille chaque VM."""
import argparse
import hashlib
import json
import os
import re
import time
from pathlib import Path

from demarrage import BUILD, ROOT, scenario_vm


def fields(line):
    return {key: int(value) for key, value in re.findall(r"(\w+)=(\d+)\b", line)}


def campaign(arch, mode="test", seconds=0, ram=256):
    target = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}[arch]
    kernel = Path(os.environ.get("NOVA_KERNEL_ELF", ROOT / "target" / target / "release" / "novaos"))
    result = dict(architecture=arch, mode=mode, ram_mio=ram,
                  elf_sha256=hashlib.sha256(kernel.read_bytes()).hexdigest())
    suffix = f"-{seconds}s" if seconds else f"-{ram}"

    def check(vm, directory, log):
        watchdog = time.monotonic() + 120
        start = bench = baseline = workload = pressure_start = None
        measured, histograms, cases, phases = [], {}, {}, {}
        offset, fragment, period, rounds = 0, b"", 0, 0
        while vm.poll() is None and time.monotonic() < watchdog:
            if log.exists():
                with log.open("rb") as stream:
                    stream.seek(offset)
                    fragment += stream.read()
                    offset = stream.tell()
            lines = fragment.split(b"\n")
            fragment = lines.pop()
            for raw in lines:
                line = raw.decode("utf8", errors="replace").strip()
                if any(err in line for err in ("Panique du noyau", "Erreur fatale", "Exception noyau")
                       ) or line.startswith(("#PF adresse=", "ABORT adresse=")):
                    raise RuntimeError(line)
                now = time.monotonic()
                if line.startswith("RAM phase="):
                    phases[line.split()[1][6:]] = fields(line)
                if not line.startswith("IPC "):
                    continue
                watchdog = now + 120
                if start is None:
                    if mode == "endurance":
                        assert line == "IPC endurance debut"
                    start = now
                elif line == "IPC endurance debut":
                    raise RuntimeError("Redémarrage pendant la campagne.")
                if line.startswith("IPC cas="):
                    cases[line.split()[1][4:]] = fields(line)["ns"]
                if line.startswith("IPC trace "):
                    result.setdefault("traces", []).append(fields(line))
                if line.startswith("IPC T09 "):
                    r = fields(line)
                    assert r["repetitions"] == 10000 and r["ordres"] == 5 and r["preemptions"] > 0
                    result["courses"] = r
                if line.startswith("IPC debut mode="):
                    if " charge=1 " in line and pressure_start is None:
                        pressure_start = now
                    assert bench is None
                    bench = now
                if line.startswith("IPC fin mode="):
                    name, pressure, series, count = re.fullmatch(
                        r"IPC fin mode=(\w+) charge=(\d+) serie=(\d+) tours=(\d+)", line).groups()
                    workload = f"{name}-{pressure}"
                    if int(series):
                        assert int(count) == 10000 and now > bench
                        measured.append(dict(mode=name, charge=int(pressure), serie=int(series),
                                             tours=int(count), secondes=now - bench))
                    else:
                        assert int(count) == 1000
                    bench = None
                if line.startswith("IPC saturation T17"):
                    workload = "revocation"
                if line.startswith("IPC hist "):
                    histograms.setdefault(workload, {})[line.split()[2][5:]] = fields(line)
                if line.startswith("IPC T17 "):
                    result["compteur_hz"] = fields(line)["compteur_hz"]
                if line.startswith("IPC contention "):
                    result["concurrence"] = fields(line)
                    result["concurrence"]["fenetre_hote_s"] = now - pressure_start
                if line.startswith("IPC endurance periode="):
                    data = fields(line)
                    p, n, free = (data[k] for k in ("periode", "cycles", "libres"))
                    assert p == period + 1 and n > 0 and baseline in (None, free), line
                    baseline, period, rounds = free, p, rounds + n
                    if now - start >= seconds:
                        result.update(secondes=round(now-start, 3), cycles=rounds, periodes=p, libres=free)
                        return
                if line != "IPC validation OK" or mode == "endurance":
                    continue
                if mode == "perf":
                    assert len(measured) == 60 and len(histograms) == 13
                    assert histograms["revocation"]["revocation"]["n"] == 100
                    for name, metrics in histograms.items():
                        if name != "revocation":
                            assert metrics["aller_retour"]["n"] == 50000
                        assert metrics["irq"]["p99"] < 4_000_000
                    total = sum(v["secondes"] for v in measured if v["mode"] == "DATA128" and not v["charge"])
                    assert 100000 / total >= 250 and histograms["DATA128-0"]["aller_retour"]["p99"] <= 16_000_000
                    result.update(series=measured, histogrammes=histograms)
                if mode == "races":
                    assert "courses" in result and len(result["traces"]) == 30
                if mode == "test":
                    covered = {t for name in cases for t in name.split("/")}
                    assert covered == ({f"T{i:02}" for i in range(1, 19)} - {"T09", "T17"}) | {"lots"}
                    assert phases["ipc_repos"] == phases["ipc_detruits"] == phases["ipc_reprise"]
                    assert phases["ipc_repos"]["ipc"] == 0 and phases["ipc_repos"]["retenu"] <= 2048
                    assert phases["ipc_saturation"]["ipc"] + 3 <= 128
                    result.update(cas_ns=cases, phases=phases)
                result["secondes"] = round(now-start, 3)
                return
            time.sleep(0.002 if mode == "perf" else 0.1)
        raise RuntimeError(f"VM interrompue ou sans progression (période {period}).")

    scenario_vm(arch, "ipc-" + mode, check, suffix, NOVA_RAM=str(ram))
    (BUILD / f"ipc-{arch}-{mode}{suffix}.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["test", "races", "perf", "endurance"])
    parser.add_argument("--seconds", type=int, default=7200)
    parser.add_argument("--ram", choices=(128, 256), type=int, default=256)
    args = parser.parse_args()
    if args.seconds <= 0:
        parser.error("Durée positive requise.")
    campaign(os.environ["NOVA_ARCH"], args.mode, args.seconds if args.mode == "endurance" else 0, args.ram)
