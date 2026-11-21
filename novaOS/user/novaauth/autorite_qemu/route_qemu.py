"""Exerce la possession de la clé dans deux processus invités. Volume de laboratoire historique."""

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "service"))
from paquet_qemu import Serial
from shell_qemu import digest, prepare_vm


def run(arch, sector, iso, genesis, output):
    args, env, disk, local_authority = prepare_vm(
        arch, sector, iso, genesis, output, ""
    )
    log = output / "invite.serial"
    result = dict(
        arch=arch,
        secteur=sector,
        iso=digest(iso),
        p9_initial=digest(disk),
        autorite_locale_initiale=digest(local_authority),
        succes=False,
    )
    try:
        with (
            log.open("xb") as trace,
            subprocess.Popen(
                args,
                env=env,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                bufsize=0,
            ) as vm,
        ):
            terminal = Serial(vm, trace)
            try:
                terminal.expect(
                    rb"NAUTH incarnation preuve invitee OK substitutions=3 rejeu=refuse",
                    90,
                )
                terminal.send(b"true; echo NA_ROUTE_$?__FIN\n")
                terminal.expect(rb"NA_ROUTE_0__FIN\r?\n", 30)
                terminal.expect(rb"nsh> ", 30)
                terminal.send(b"exit\n")
                terminal.expect(rb"absents=0x0 muets=0x0", 45)
            finally:
                if vm.poll() is None:
                    vm.terminate()
                try:
                    vm.wait(5)
                except subprocess.TimeoutExpired:
                    vm.kill()
                    vm.wait()
                trace.write(vm.stdout.read())
        data = log.read_bytes()
        participants = {}
        for service in (22, 23):
            match = re.search(
                rb"NAUTH phase=23 pid=(\d+) service="
                + str(service).encode()
                + rb" roots=(\d+)",
                data,
            )
            if not match or match[2] != b"4":
                raise ValueError(f"budget IPC du service {service}")
            pid = int(match[1])
            participants[service] = pid
            saturation = re.search(
                rb"NAUTH saturation pid="
                + match[1]
                + rb" avant=(\([^)]*\)) apres=(\([^)]*\)) statut=Err\(6\)",
                data,
            )
            if not saturation or saturation[1] != saturation[2]:
                raise ValueError(
                    f"allocation résiduelle après saturation du service {service}"
                )
        if (
            participants[22] == participants[23]
            or b"BLOC session terminee ok=true" not in data
        ):
            raise ValueError("isolation ou arrêt du stockage")
        result.update(succes=True, processus=participants)
    finally:
        if log.is_file():
            result["serial"] = digest(log)
        (output / "resultat.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("architecture", choices=("aarch64", "x86_64"))
    parser.add_argument("secteur", type=int, choices=(512, 4096))
    for name in ("iso", "genese", "sortie"):
        parser.add_argument(name, type=Path)
    a = parser.parse_args()
    run(a.architecture, a.secteur, a.iso, a.genese, a.sortie)
