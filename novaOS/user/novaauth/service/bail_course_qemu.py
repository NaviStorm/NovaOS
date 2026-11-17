"""Ferme un bail en concurrence avec l'échéance réelle du noyau. Le même shell se réauthentifie."""
import argparse
import json
import subprocess
from pathlib import Path

from bail_qemu import decisions, one
from flux_qemu import identites
from paquet_qemu import Serial, digest, prepare_vm


def run(arch, sector, iso, seed, out):
    cmd, env, disk, authority = prepare_vm(arch, sector, iso, seed, out, "")
    r = dict(architecture=arch, secteur=sector, iso=digest(iso), p9_initial=digest(disk),
             autorite_initiale=digest(authority), succes=False)
    log = out / "invite.serial"
    with log.open("xb") as raw, subprocess.Popen(cmd, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0) as vm:
        term = Serial(vm, raw)
        try:
            term.expect(rb"nsh> ")
            r["acteurs"] = identites(term.data)
            term.submit("redir-test course-fermeture; echo NXCOURSE_$?__FIN")
            term.expect(rb"NXBAIL expiration pendant fermeture", 100)
            term.expect(rb"NXCOURSE_0__FIN\r?\n")
            term.expect(rb"nsh> ")
            first = decisions(term.data)
            start = one(first, "NSHOPEN1")
            closes = [v for v in first if v["op"] == "NSHCLOS1"]
            if len(closes) != 2 or any(v["retour"] != 0 or v["fin_ms"] < start["echeance_ms"] for v in closes):
                raise ValueError("fermetures concurrente et tardive")
            begin = term.offset
            term.submit("true")
            term.expect("saisie refusée : accès refusé".encode())
            term.expect(rb"nsh> ")
            if decisions(term.data[begin:term.offset]):
                raise ValueError("authentification implicite")
            begin = term.offset
            term.submit("session reauthentifier")
            term.expect("NovaAuth : nouvelle session authentifiée".encode())
            term.expect(rb"nsh> ")
            second = one(decisions(term.data[begin:term.offset]), "NSHOPEN1")
            if second["echeance_ms"] - 60_000 < start["echeance_ms"]:
                raise ValueError("échéance ou nouvelle session")
            term.submit("true; echo NXLIVE_$?__FIN")
            term.expect(rb"NXLIVE_0__FIN\r?\n")
            term.expect(rb"nsh> ")
            term.send(b"exit\n")
            term.expect(rb"absents=0x0 muets=0x0")
            if b"BLOC session terminee ok=true" not in term.data:
                raise ValueError("drainage")
            r.update(fermetures=closes, premiere=start, seconde=second, succes=True)
        except (ValueError, EOFError, TimeoutError, OSError) as err:
            r["erreur"] = str(err)
        finally:
            if vm.poll() is None:
                vm.terminate()
            vm.wait(10)
            raw.write(vm.stdout.read())
    r.update(serial=digest(log), p9_final=digest(disk), autorite_finale=digest(authority))
    (out/"resultat.json").write_text(json.dumps(r,ensure_ascii=False,indent=2)+"\n")
    print(arch, sector, r["succes"], r.get("erreur",""), flush=True)
    return r["succes"]


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch", choices=("aarch64","x86_64"))
    p.add_argument("secteur",type=int,choices=(512,4096))
    for name in ("iso","genese","sortie"):
        p.add_argument(name,type=Path)
    a=p.parse_args()
    raise SystemExit(0 if run(a.arch,a.secteur,a.iso,a.genese,a.sortie) else 1)
