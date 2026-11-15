"""Démarre le pilote EDU dans QEMU. Conserve l'oracle PCI, les fautes et les empreintes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

from demarrage import BUILD, ROOT, qmp


CASES = {"normal", "refus", "ecriture", "hors-page", "retrait", "pendante", "muet",
         "acquit-avant", "retrait-avant", "mort-irq", "irq-attente"}
LAB_PAGE = 0x700000


def dump(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_config(vm, arch, offset, slot=6, function=0):
    # Le banc interroge uniquement les en-têtes et les BAR du bus racine annoncé.
    if not (0 <= slot < 32 and 0 <= function < 8 and 0 <= offset <= 252 and offset % 4 == 0):
        raise ValueError((slot, function, offset))
    root = 0xE0000000 if arch == "x86_64" else 0x4010000000
    addr = root + (slot << 15) + (function << 12) + offset
    reply = qmp(vm, "human-monitor-command", **{"command-line": f"xp /1wx {addr:#x}"})
    value = re.fullmatch(r"\s*([0-9a-fA-F]+):\s*0x([0-9a-fA-F]+)\s*", reply)
    if not value or int(value[1], 16) != addr:
        raise RuntimeError(f"Lecture QMP de la configuration de la carte EDU en échec à {addr:#x}: {reply}")
    return int(value[2], 16)


def oracle(vm, arch, cpus):
    roots = qmp(vm, "query-pci")
    devices = [d for root in roots for d in root["devices"] if d["qdev_id"] == "edu-phase2"]
    if len(devices) != 1:
        raise RuntimeError("Le slot de laboratoire doit contenir exactement une carte EDU.")
    dev = devices[0]
    identity = dev["id"]
    if ((dev["bus"], dev["slot"], dev["function"]) != (0, 6, 0)
        or (identity["vendor"], identity["device"]) != (0x1234, 0x11e8)
        or dev["irq_pin"] != 1 or len(dev["regions"]) != 1):
        raise RuntimeError(f"Identité ou ressources PCI inattendues : {dev}")
    bar = dev["regions"][0]
    if (bar["bar"], bar["size"], bar["type"], bar["mem_type_64"], bar["prefetch"]) != (
        0, 1 << 20, "memory", False, False,
    ):
        raise RuntimeError(f"Géométrie PCI inattendue : {bar}")
    cpus_qmp = [cpu["cpu-index"] for cpu in qmp(vm, "query-cpus-fast")]
    if sorted(cpus_qmp) != list(range(cpus)):
        raise RuntimeError(f"vCPU QMP différents du profil : {cpus_qmp}")
    if read_config(vm, arch, 0) != 0x11e81234:
        raise RuntimeError("Lecture indépendante de l'identité PCI incohérente.")
    command = read_config(vm, arch, 4) & 0xffff
    if command & 7:
        raise RuntimeError(f"Le décodage mémoire/E/S ou le bus mastering d’EDU reste actif : {command:#x}")
    assigned = read_config(vm, arch, 0x10)
    if assigned & 0xfffff or not assigned or bar["address"] not in (-1, assigned):
        raise RuntimeError(f"BAR brute et décodage QEMU incohérents : {assigned:#x}, {bar}")
    inventory = {}
    for root in roots:
        for device in root["devices"]:
            if device["bus"] or "pci_bridge" in device:
                raise RuntimeError("Le profil du pilote exige des fonctions directement sur le bus zéro.")
            slot, function = device["slot"], device["function"]
            for region in device["regions"]:
                index = region["bar"]
                if index >= 6:
                    continue
                off = 0x10 + 4 * index
                address = read_config(vm, arch, off, slot, function)
                if region["type"] == "io":
                    address &= ~3
                    kind = "Io"
                else:
                    address &= ~15
                    kind = "Memory64" if region["mem_type_64"] else "Memory32"
                    if kind == "Memory64":
                        address |= read_config(vm, arch, off + 4, slot, function) << 32
                if region["address"] not in (-1, address):
                    raise RuntimeError(f"BAR décodée et adresse configurée divergentes pour {device['id']}")
                if address:
                    key = f"0000:00:{slot:02x}.{function} n={index}"
                    inventory[key] = [address, region["size"], kind, region.get("prefetch", False)]
    caps = []
    ptr = read_config(vm, arch, 0x34) & 0xff
    seen = set()
    while ptr:
        if not 0x40 <= ptr <= 0xfc or ptr % 4 or ptr in seen:
            raise RuntimeError(f"Liste de capacités EDU malformée à {ptr:#x}.")
        seen.add(ptr)
        cap = read_config(vm, arch, ptr)
        caps.append({"offset": ptr, "id": cap & 0xff, "controle": cap >> 16})
        ptr = (cap >> 8) & 0xff
    if not any(cap["id"] == 5 and not cap["controle"] & 1 for cap in caps):
        raise RuntimeError("La capacité MSI d'EDU doit être présente et désarmée sur ce profil INTx.")
    return {"appareil": dev, "pci_qmp": roots, "capacites_edu": caps, "bar_attribuees": inventory,
            "adresse_bar0": assigned, "commande_pci_finale": command,
            "vcpus": cpus_qmp, "version": qmp(vm, "query-version")}


def verify(serial, arch, cpus, reference):
    if "PILOTE bilan ok=true" not in serial or "PILOTE ressources ipc=0 cadres_restitues=true" not in serial:
        raise RuntimeError(f"Bilan ou restitution absents.\n{serial[-1700:]}")
    cases = re.findall(r"^PILOTE cas=([a-z-]+) ok=true\b", serial, re.MULTILINE)
    if set(cases) != CASES or len(cases) != len(CASES):
        raise RuntimeError(f"Cas manquants ou répétés : {cases}")
    bars = {}
    for key, base, size, kind, prefetch in re.findall(
        r"^LAB-BAR (\S+ n=\d+) base=(0x[0-9a-f]+) taille=(\d+) type=(\w+) prefetch=(true|false)",
        serial, re.MULTILINE,
    ):
        if key in bars:
            raise RuntimeError(f"BAR répétée dans l'inventaire : {key}")
        bars[key] = [int(base, 16), int(size), kind, prefetch == "true"]
    if bars != reference["bar_attribuees"]:
        raise RuntimeError(f"BAR invitées {bars} différentes des BAR de QEMU {reference['bar_attribuees']}")
    closure = re.search(r"^EDU PCI retrait 0000:00:06\.0 BAR0=(0x[0-9a-f]+) commande=(0x[0-9a-f]+)",
                        serial, re.MULTILINE)
    if not closure or (int(closure[1], 16), int(closure[2], 16)) != (
        reference["adresse_bar0"], reference["commande_pci_finale"],
    ):
        raise RuntimeError("Le retrait PCI vu par le noyau diffère de la relecture QMP.")
    placement = re.search(r"PILOTE retrait cpu=(\d+) pilote_cpu=(\d+)", serial)
    if not placement:
        raise RuntimeError("Le processeur du retrait n'a pas été mesuré.")
    retired, driver_cpu = map(int, placement.groups())
    if not (0 <= retired < cpus and 0 <= driver_cpu < cpus and (cpus == 1 or retired != driver_cpu)):
        raise RuntimeError(f"Placement du retrait non conforme : {placement[0]}")
    level = "EL0" if arch == "aarch64" else "Ring3"
    process = re.compile(rf"^PROC pid=(\d+) niveau={level} pc=0x[0-9a-f]+ ([^\r\n]+)", re.MULTILINE)
    fault_groups = {
        "hors-page": (LAB_PAGE + 4096, "lecture", "absente"),
        "mort-irq": (LAB_PAGE + 4096, "lecture", "absente"),
        "ecriture": (LAB_PAGE + 4, "ecriture", "protection"),
        **{name: (LAB_PAGE, "lecture", "absente")
           for name in ("retrait", "pendante", "acquit-avant", "retrait-avant")},
    }
    start = 0
    seen = set()
    processes = {}
    for case in re.finditer(r"^PILOTE cas=([a-z-]+) ok=true\b", serial, re.MULTILINE):
        name = case[1]
        section = serial[start:case.start()]
        start = case.end()
        pair = sorted(process.findall(section), key=lambda row: int(row[0]))
        if len(pair) != 2 or pair[0][0] == pair[1][0] or any(pid in seen for pid, _ in pair):
            raise RuntimeError(f"Le pilote puis son gestionnaire doivent avoir deux PID uniques pour {name}: {pair}")
        seen.update(pid for pid, _ in pair)
        (driver, outcome), (manager, collected) = pair
        if collected != "sortie=0":
            raise RuntimeError(f"Le gestionnaire du pilote {driver} termine avec {collected} pour {name}.")
        if name == "normal":
            milestones = ["PILOTE DMA physique refuse avant demarrage",
                          f"DEBUG pid={driver} EDU ID=010000ed liveness=ok IRQ=2 acquits=2",
                          f"PROC pid={driver} niveau=", f"PROC pid={manager} niveau=",
                          "PILOTE IRQ sans bail entrees=1 notifications=0 drain=true",
                          "PILOTE IRQ tardive sans destinataire confirmee"]
            positions = [section.find(marker) for marker in milestones]
            ok = (outcome == "sortie=0" and all(section.count(marker) == 1 for marker in milestones)
                  and positions == sorted(positions))
        elif name == "irq-attente":
            ok = outcome == "sortie=0" and f"PILOTE IRQ levee receveur bloque pid={driver}" in section
        elif name == "refus":
            ok = outcome == "sortie=112"  # le pilote rapporte 100 + statut public REFUSED.
        elif name == "muet":
            ok = "faute=SUPERVISION " in outcome and "cause=delai" in outcome
        else:
            addr, access, cause = fault_groups[name]
            trap = "ABORT" if arch == "aarch64" else "#PF"
            ok = f"faute={trap} adresse={addr:#x} acces={access} cause={cause} " in outcome
        if name == "mort-irq":
            ok = ok and f"PILOTE mort avec IRQ non acquittee pid={driver}" in section
        if name in ("acquit-avant", "retrait-avant"):
            first = str(name == "acquit-avant").lower()
            ok = ok and f"PILOTE course {name} acquit_en_file=true acquit_execute={first}" in section
        if not ok:
            raise RuntimeError(f"Issue utilisateur incorrecte pour {name}, PID {driver}: {outcome}")
        processes[name] = {"pilote": int(driver), "issue": outcome, "gestionnaire": int(manager),
                           "issue_gestionnaire": collected}
    return {"cas": cases, "cpu_retrait": retired, "cpu_pilote": driver_cpu, "processus": processes}


def run(arch, cpus, iso, elf):
    directory = Path(tempfile.mkdtemp(prefix=f"pilote-edu-{arch}-{cpus}cpu-", dir=BUILD))
    serial = directory / "serie.log"
    argv = ["sh", str(ROOT / "scripts/qemu.sh"), "-display", "none", "-monitor", "none",
            "-serial", f"file:{serial}", "-qmp", "stdio",
            "-device", "edu,id=edu-phase2,bus=pcie.0,addr=06.0"]
    env = {**os.environ, "NOVA_ARCH": arch, "NOVA_VCPUS": str(cpus), "NOVA_VGA": "none",
           "NOVA_GIC": "2", "NOVA_ISO": str(iso)}
    print(f"Preuves {directory}", flush=True)
    with (directory / "qemu.log").open("wb") as errors, subprocess.Popen(
        argv, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors, bufsize=0,
    ) as vm:
        try:
            qmp(vm, "qmp_capabilities")
            end = time.monotonic() + 80
            while vm.poll() is None and time.monotonic() < end:
                report = serial.read_text(errors="replace") if serial.exists() else ""
                if re.search(r"^PILOTE bilan ok=(true|false)\b", report, re.MULTILINE) or "Panique du noyau" in report:
                    break
                time.sleep(0.04)
            else:
                raise RuntimeError(f"Diagnostic non terminé dans QEMU, consulter {serial}.")
            qmp(vm, "stop")
            ref = oracle(vm, arch, cpus)
            dump(directory / "qmp.json", ref)
            result = verify(report, arch, cpus, ref)
            qmp(vm, "quit")
            if vm.wait(timeout=5):
                raise RuntimeError("Sortie QEMU anormale.")
        except Exception:
            shutil.copyfile(iso, directory / "echec.iso")
            raise
        finally:
            if vm.poll() is None:
                vm.kill()
                vm.wait()
    dump(directory / "bilan.json", {"architecture": arch, "vcpus": cpus, **result,
         "bar_verifiees": len(ref["bar_attribuees"]), "bar_edu": ref["adresse_bar0"],
         "commande_pci_finale": ref["commande_pci_finale"], "qemu": ref["version"],
         "sha256_elf": sha256(elf), "sha256_iso": sha256(iso), "sha256_serie": sha256(serial),
         "sha256_qmp": sha256(directory / "qmp.json")})
    print(f"OK — {arch}, {cpus} vCPU, {len(CASES)} scénarios du pilote EDU.", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vcpus", type=int, choices=(1, 2, 4), nargs="+", default=(1, 2, 4))
    args = parser.parse_args()
    architecture = os.environ["NOVA_ARCH"]
    target = {"aarch64": "aarch64-unknown-none-softfloat", "x86_64": "x86_64-unknown-none"}[architecture]
    elf = Path(os.environ.get("NOVA_KERNEL_ELF", ROOT / "target" / target / "release/novaos"))
    BUILD.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="iso-edu-", dir=BUILD) as stage:
        iso = Path(stage) / "pilote.iso"
        subprocess.run(["sh", ROOT / "scripts/iso.sh", "materiel-pilote-qemu", iso],
                       check=True, stdout=subprocess.DEVNULL)
        for n in args.vcpus:
            run(architecture, n, iso, elf)
