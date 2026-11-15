"""Interruption QEMU au premier FLUSH d'une rotation de configuration P9 déjà initialisée."""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import time
from types import SimpleNamespace

from bloc import qmp
from cli import Terminal
from services_configuration import commande, etat, generation
from services_configuration_courses import digest, verify_slots


def trial(arch, sector, cpus, source, out, tool):
    out.mkdir(parents=True, exist_ok=False)
    initial = digest(source)
    image = out / "interruption.raw"
    shutil.copyfile(source, image)
    assert digest(image) == initial

    def inspect(disk):
        return subprocess.check_output([tool, "inspect", disk, str(sector)])

    original_records = inspect(image)
    verify_slots(original_records)
    os.environ["NOVA_VCPUS"] = str(cpus)
    serial = out / "coupure.serial"
    with ExitStack() as stack:
        local, remote = socket.socketpair()
        stack.enter_context(local)
        stack.enter_context(remote)
        local.settimeout(10)
        channel = stack.enter_context(local.makefile("rwb", buffering=0))
        monitor = SimpleNamespace(stdin=channel, stdout=channel)
        vm = stack.enter_context(Terminal(arch, sector, image, "config-coupure", profil_config=True,
                                          qmp_bloc=remote.fileno(), journal=serial, diagnostic=True))
        remote.close()
        log = stack.enter_context((out / "qmp.jsonl").open("x"))

        def query(op, **args):
            print(json.dumps(dict(t=time.monotonic_ns(), op=op, args=args)), file=log, flush=True)
            result = qmp(monitor, op, **args)
            print(json.dumps(dict(t=time.monotonic_ns(), result=result)), file=log, flush=True)
            return result

        def breakpoint(command):
            result = query("human-monitor-command", **{"command-line": f'qemu-io d "{command}"'})
            assert result == "", result

        def counts():
            stats = next(s["stats"] for s in query("query-blockstats") if s["device"] == "d")
            return {k: stats[k] for k in ("wr_operations", "flush_operations",
                                          "failed_wr_operations", "failed_flush_operations")}

        query("qmp_capabilities")
        vm.until(b"nsh> ")
        for service in (1, 2, 9, 10):
            etat(vm, service, "RUNNING")
        before = counts()
        status = commande(vm, "service status 9")
        assert generation(status, "Effective") == generation(status, "Stored") == 2
        assert "Autostart: enabled" in status and "Pending restart: no" in status
        assert counts() == before, "la consultation a produit une écriture"
        breakpoint("break flush_to_disk config")
        vm.vm.stdin.write(b"service disable 9\n")
        vm.until(b"\r\n")
        # Laisser l'invité soumettre sa commande avant l'appel HMP synchrone.
        # Le délai n'établit ni la remise du FLUSH ni le point de coupure.
        time.sleep(0.6)
        breakpoint("wait_break config")
        at_cut = counts()
        assert at_cut["wr_operations"] > before["wr_operations"]
        assert at_cut["flush_operations"] == before["flush_operations"]
        assert not any(at_cut[k] for k in ("failed_wr_operations", "failed_flush_operations"))
        vm.vm.kill()
        assert vm.vm.wait(timeout=10) == -signal.SIGKILL
    # La fermeture du terminal draine le dernier paquet UART après SIGKILL.
    assert "enregistré=disabled".encode() not in serial.read_bytes()
    cut_sha = digest(image)
    assert inspect(image) == original_records, "racines, contenus ou sentinelle modifiés"
    recovery = out / "reprise.raw"
    shutil.copyfile(image, recovery)
    assert digest(recovery) == cut_sha
    restart_log = out / "reprise.serial"
    with Terminal(arch, sector, recovery, "config-coupure-reprise", journal=restart_log) as vm:
        vm.until(b"nsh> ")
        status = etat(vm, 9, "RUNNING")
        assert generation(status, "Effective") == generation(status, "Stored") == 2
        assert "Autostart: enabled" in status and "Pending restart: no" in status
        etat(vm, 10, "RUNNING")
        assert "1397118542" in commande(vm, "service info 9")
        assert "NovaOS" in commande(vm, "cat /documents/exemple.txt")
        vm.exit()
        vm.until(b"SMP arret motif=normal")
    assert inspect(recovery) == original_records
    assert digest(source) == initial and digest(image) == cut_sha
    data = dict(architecture=arch, secteur=sector, vcpus=cpus, generation_reprise=2,
                image_originale=initial, image_coupee=cut_sha, image_reprise=digest(recovery),
                premier_flush_suspendu=True, avant=before, suspension=at_cut,
                serie=digest(serial), serie_reprise=digest(restart_log),
                noyau=digest(Path(os.environ["NOVA_KERNEL_ELF"])), oracle=digest(tool))
    (out / "resultats.json").write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("x86_64", "aarch64"))
    p.add_argument("secteur", type=int, choices=(512, 4096))
    p.add_argument("vcpus", type=int, choices=(1, 2, 4))
    p.add_argument("source", type=Path, help="copie P9 de qualification contenant les générations 1 et 2")
    p.add_argument("preuves", type=Path)
    p.add_argument("oracle", type=Path)
    a = p.parse_args()
    trial(a.arch, a.secteur, a.vcpus, a.source.resolve(strict=True), a.preuves.resolve(),
          a.oracle.resolve(strict=True))
