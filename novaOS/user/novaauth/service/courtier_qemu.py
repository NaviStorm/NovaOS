"""Exerce les transferts du courtier depuis le vrai shell et archive les refus."""
import argparse
import json
from pathlib import Path
import re
import subprocess

from paquet_qemu import Serial
from shell_qemu import digest, prepare_vm


def run(a):
    args, env, disk, authority = prepare_vm(a.arch, a.sector, a.iso, a.genesis, a.output, "" if a.tickets else "neuf-")
    report = dict(architecture=a.arch, secteur=a.sector, iso=digest(a.iso),
                  p9_initial=digest(disk), autorite_initiale=digest(authority), essais=[], succes=False)
    trace = a.output / "invite.serial"
    with trace.open("xb") as log, subprocess.Popen(
        args, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, bufsize=0
    ) as vm:
        serial = Serial(vm, log)
        try:
            serial.expect(rb"nsh> ")
            def check(line, expected, marker=None):
                code, response = serial.command(line)
                report["essais"].append(dict(commande=line, statut=code, attendu=expected))
                if code != expected or (marker is not None and marker not in response):
                    raise ValueError(f"{line} statut {code}, attendu {expected}, marqueur {marker!r}")
            check("true", 0)
            if a.tickets and not a.panne:
                check("ticket-test recu cut -f1,3 donnees.tsv", 5, b"collecte unique")
                check("ticket-test expire cut",0,b"ticket expire")
                check("ticket-test revoque cut",0,b"ticket revoque")
            elif a.panne:
                probe = "ticket-test recu cut" if a.tickets else "relay-test"
                code, _ = serial.command(probe)
                report["essais"].append(dict(commande=probe, statut=code))
                if code not in (5,8,10,11):
                    raise ValueError(f"statut après panne {code}")
                pattern = (b"NXRELAY panne en_attente=true feuilles_retirees=0" if a.panne=="avant"
                           else b"NXRELAY panne en_attente=false feuilles_retirees=" + (b"0" if a.tickets else b"1"))
                if pattern not in serial.data or b"NXRELAY apres retrait tickets_shell=0" not in serial.data:
                    raise ValueError("phase observée de la panne")
                # Cette commande relit effectivement le stockage via la session NovaAuth.
                if a.tickets:
                    if b"NXEXEC pid=" in serial.data:
                        raise ValueError("publication inattendue d'un enfant")
                    check("install --inspect cut",0,b"NXPK objet cut")
                else:
                    check("install --inspect temoin",126)
            else:
                for _ in range(2):
                    check("relay-test", 0, b"NXRELAY flux exact")
            check("true", 0)
            serial.send(b"exit\n")
            serial.expect(rb"absents=0x0 muets=0x0")
            roots = re.findall(rb"NXRELAY branche shell=(\d+) verificateur=(\d+) courtier=(\d+) racines_auth=(\d+) racines_courtier=(\d+)", serial.data)
            if len(roots) != 1 or roots[0][3:] != (b"4", b"4") or len(set(roots[0][:3])) != 3:
                raise ValueError("topologie et quotas")
            if b"NXRELAY aucun droit de creation ou duplication" not in serial.data or b"BLOC session terminee ok=true" not in serial.data:
                raise ValueError("refus ou drainage absent")
            report["pid"] = [int(v) for v in roots[0][:3]]
            report["racines"] = [int(v) for v in roots[0][3:]]
            report["succes"] = True
        except (OSError, TimeoutError, ValueError, EOFError) as exc:
            report["erreur"] = str(exc)
        finally:
            if vm.poll() is None:
                vm.terminate()
            vm.wait(10)
            log.write(vm.stdout.read())
    report.update(trace=digest(trace), p9_final=digest(disk), autorite_finale=digest(authority))
    (a.output / "resultat.json").write_text(json.dumps(report, ensure_ascii=False, indent=2)+"\n")
    print(json.dumps(report, ensure_ascii=False), flush=True)
    return report["succes"]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("arch", choices=("x86_64", "aarch64"))
    parser.add_argument("sector", type=int, choices=(512,4096))
    parser.add_argument("--tickets",action="store_true")
    parser.add_argument("--panne",choices=("avant","apres"))
    for item in ("iso","genesis","output"):
        parser.add_argument(item,type=Path)
    raise SystemExit(0 if run(parser.parse_args()) else 1)
