"""Chronologie de deux admissions par nom dans un seul invité et sur une seule installation."""
import argparse
import json
import re
import subprocess
import time
from pathlib import Path

from flux_qemu import composants, identites
from paquet_qemu import Serial, digest, prepare_vm

MARK = re.compile(rb"NXBAIL op=(\w+) tag=(\d+) decision=(\d+) valide=(\d+) echeance=(\d+) retour=(\d+) fin=(\d+)")
TSV = b"A\tB\tC\nD\tE\tF\n"


def decisions(data):
    return [dict(op=m[1].decode(), tag=int(m[2]), decision_ms=int(m[3]), valide=bool(int(m[4])),
                 echeance_ms=int(m[5]), retour=int(m[6]), fin_ms=int(m[7])) for m in MARK.finditer(data)]


def one(rows, op):
    found = [row for row in rows if row["op"] == op]
    if len(found) != 1:
        raise ValueError((op, found))
    return found[0]


def run(a):
    start = time.monotonic_ns()
    argv, env, disk, authority = prepare_vm(a.arch, a.secteur, a.iso, a.genese, a.sortie, "")
    r = dict(arch=a.arch, secteur=a.secteur, iso=digest(a.iso), p9_initial=digest(disk),
             autorite_initiale=digest(authority), paquet=digest(a.paquet), commande_qemu=argv,
             etapes=[], controles={}, succes=False)
    trace = a.sortie / "invite.serial"
    def ms():
        return (time.monotonic_ns() - start) // 1_000_000
    def require(condition, message):
        if not condition:
            raise ValueError(message)
    with trace.open("xb") as out, subprocess.Popen(argv, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0) as vm:
        term = Serial(vm, out)
        def step(label, line, payload=None, watch=(), timeout=55, drain=False):
            begin = term.offset
            row = dict(etape=label, commande=line, envoi_hote_ms=ms(), observations=[])
            r["etapes"].append(row)
            suffix = "; echo NXB_$?__FIN" + ("; exit 0" if drain else "")
            term.submit(line + suffix, payload)
            for pattern in watch:
                matched = term.expect(pattern, timeout)
                row["observations"].append(dict(ligne=matched[0].decode(), reception_hote_ms=ms()))
            status = term.expect(rb"NXB_(\d+)__FIN\r?\n", timeout)
            if drain:
                term.expect(rb"absents=0x0 muets=0x0")
                require(b"BLOC session terminee ok=true" in term.data, "drainage stockage")
            else:
                term.expect(rb"nsh> ")
            data = bytes(term.data[begin:term.offset])
            row.update(statut=int(status[1]), invite_ms=ms(), decisions=decisions(data))
            return data, row
        try:
            term.expect(rb"nsh> ")
            r["preparation_hote_ms"] = ms()
            r["composants_avant"] = composants(a.iso)
            r["acteurs"] = identites(term.data)
            marker = term.offset
            absent, row = step("absence", "temoin", watch=[rb"NXBAIL op=NSHOPEN1[^\r\n]*\r?\n"])
            require(row["statut"] == 127 and b"commande inconnue" in absent, "présence initiale")
            opening = one(row["decisions"], "NSHOPEN1")
            # Le bind du courtier inspecte le bail dans open, après sa création.
            # relay_deadline est calculé sur le même tick que Sessions.open(..., 60_000).
            r["creation_bail_ms"] = opening["echeance_ms"] - 60_000
            require(opening["valide"] and 0 < r["creation_bail_ms"] <= opening["decision_ms"] <
                    opening["echeance_ms"], "bail initial")
            blob = a.paquet.read_bytes()
            _, row = step("installation", f"install --hex {len(blob)}", blob)
            require(row["statut"] == 0, "installation")
            data, row = step("admission_valide", "temoin", watch=[
                rb"NXBAIL op=NSHEXEND[^\r\n]*\r?\n", rb"PROC pid=\d+ [^\r\n]*sortie=0\r?\n"])
            grants = one(row["decisions"], "NSHEXEND")
            frames = re.findall(rb"NXFLUX pid=(\d+) fd=(\d+) octets=(\d+) prefixe=([0-9a-f]*)",data)
            require(len(frames) == 2 and frames[0][0] == frames[1][0]
                    and frames[0][1:] == (b"1",b"12",TSV.hex().encode())
                    and frames[1][1:] == (b"2",b"0",b"") and row["statut"] == 0, "flux ou statut")
            require(grants["valide"] and grants["retour"] == 0 and 0 < grants["decision_ms"] <
                    grants["echeance_ms"] == opening["echeance_ms"], "admission hors bail")
            row.update(pid=int(frames[0][0]), stdout=TSV.hex(), stderr="")
            r["controles"]["installation_et_admission"] = True
            line = ("redir-test attendre-bail /travail/donnees.tsv; echo NXATT_$?__FIN; "
                    "temoin; echo NXEXP_$?__FIN; redir-test reauthentifier && temoin")
            data, row = step("expiration_et_nouvelle_authentification", line, timeout=160, drain=a.drainage)
            refusal = one(row["decisions"], "NSHEXBG1")
            require(b"NXATT_0__FIN\r\n" in data and b"NXEXP_126__FIN\r\n" in data
                    and not refusal["valide"] and refusal["retour"] == 5 and
                    refusal["decision_ms"] >= refusal["echeance_ms"] == opening["echeance_ms"],
                    "refus avant ticket ou identité du bail")
            end = data.index(b"NXEXP_126__FIN\r\n")
            negative = data[data.index(b"NXATT_0__FIN\r\n"):end]
            require(b"NXFLUX pid=" not in negative and b"NSHEXEND" not in negative
                    and b"BUDGET create" not in negative and b"PROC pid=" not in negative,
                    "émission ou flux après refus")
            r["controles"]["refus_avant_creation"] = True
            read = re.search(rb"NXBAIL ancien handle lecture=(\d+)", data)
            auth = re.search(rb"NXBAIL reauth fermeture=(\d+) ouverture=(\d+)", data)
            require(read and auth, "sondes de retrait et réauthentification absentes")
            row.update(ancienne_lecture=int(read[1]), fermeture=int(auth[1]), ouverture=int(auth[2]))
            r["controles"]["ancien_handle_inutilisable"] = int(read[1]) in (5, 10, 11)
            r["controles"]["nouveau_bail"] = int(auth[2]) == 0 and row["statut"] == 0 and any(
                item["op"] == "NSHOPEN1" and item["valide"] and item["echeance_ms"] > opening["echeance_ms"]
                for item in row["decisions"])
            tail = term.data[marker:]
            r["composants_apres"] = composants(a.iso)
            require(r["composants_avant"] == r["composants_apres"] and
                    not any(x in tail for x in (b"NXLOGIN route", b"NXRELAY branche")), "redémarrage")
            if a.drainage:
                r["sortie_dans_liste_deja_admise"] = True
            if r["controles"]["nouveau_bail"]:
                require(re.findall(rb"fd=1 octets=12 prefixe=([a-f0-9]+)",data[end:]) ==
                        [TSV.hex().encode()] and b"fd=2 octets=0 prefixe=\r\n" in data[end:], "relance")
                if not a.drainage:
                    term.send(b"exit\n")
                    term.expect(rb"absents=0x0 muets=0x0")
                require(b"BLOC session terminee ok=true" in term.data, "drainage")
                r["controles"]["drainage"] = True
            r["succes"] = all(r["controles"].values())
        except (ValueError, TimeoutError, OSError, EOFError) as e:
            r["erreur"] = str(e)
        finally:
            if vm.poll() is None:
                vm.terminate()
            vm.wait(10)
            out.write(vm.stdout.read())
    r.update(serial=digest(trace), p9_final=digest(disk), autorite_finale=digest(authority))
    (a.sortie/"resultat.json").write_text(json.dumps(r,ensure_ascii=False,indent=2)+"\n")
    print(a.arch, a.secteur, r["controles"], r.get("erreur",""), flush=True)
    return r["succes"]


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--drainage", action="store_true", help="terminer explicitement le shell dans la liste déjà admise et mesurer la restitution")
    p.add_argument("arch", choices=("aarch64","x86_64"))
    p.add_argument("secteur", type=int, choices=(512,4096))
    for name in ("iso","genese","paquet","sortie"):
        p.add_argument(name,type=Path)
    raise SystemExit(0 if run(p.parse_args()) else 1)
