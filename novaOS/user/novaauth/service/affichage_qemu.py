"""Recette depuis la console de nsh, avec paquets signés et copie P9 par invité."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

from paquet_qemu import Serial, digest, prepare_vm


def run(a):
    argv, env, disk, authority = prepare_vm(a.arch, a.sector, a.iso, a.genesis, a.output, "")
    record = dict(architecture=a.arch, secteur=a.sector, iso=digest(a.iso), p9_initial=digest(disk),
                  autorite_initiale=digest(authority), paquet=digest(a.package), essais=[], succes=False)
    trace = a.output / "invite.serial"
    with trace.open("xb") as log, subprocess.Popen(argv, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0) as vm:
        terminal = Serial(vm, log)
        def command(line, status, data=None):
            rc, output = terminal.command(line, data)
            record["essais"].append(dict(commande=line, statut=rc, attendu=status))
            if rc != status:
                raise ValueError((line,rc,status))
            return output
        def streams(line, status, stdout, stderr, visual=None):
            output = command(line, status)
            found = re.findall(rb'NXFLUX pid=(\d+) fd=([12]) octets=(\d+) prefixe=([0-9a-f]*)',output)
            if len(found)!=2 or found[0][0]!=found[1][0]:
                raise ValueError(("trames",line,found))
            for i, raw in enumerate([stdout,stderr]):
                pid,fd,n,prefix=found[i]
                if int(fd)!=i+1 or int(n)!=len(raw) or bytes.fromhex(prefix.decode())!=raw[:64]:
                    raise ValueError(("octets",line,found))
            record["essais"][-1].update(pid=int(pid), stdout=stdout.hex(), stderr=stderr.hex())
            if visual is not None and visual not in output.replace(b"\r\n",b"\n"):
                raise ValueError(("affichage",line))
            return int(pid)
        try:
            terminal.expect(rb'nsh> ')
            command("temoin", 127)
            packet=a.package.read_bytes()
            command("install --hex "+str(len(packet)),0,packet)
            raw=b"A\tB\tC\nD\tE\tF\n"
            visual=b"A       B       C\nD       E       F\n"
            one=streams("temoin",0,raw,b"",visual)
            two=streams("/system/bin/temoin",0,raw,b"",visual)
            if one==two:
                raise ValueError("PID réutilisé")
            args=b"\x00\x00\x03\x00a b\x02\x00-x"
            streams('nsh -c "temoin \'\' \'a b\' -x"',0,args,b"")
            pipe=command("temoin | cut -f1,3",0)
            stages=re.search(rb"NXPIPE producteur=(\d+) statut=0 lecteur=(\d+) statut=0",pipe)
            if not stages or stages[1]==stages[2]: raise ValueError(("collecte du pipe",pipe[-220:]))
            results=re.findall(rb'NXFLUX pid=(\d+) fd=([12]) octets=(\d+) prefixe=([0-9a-f]*)',pipe)
            if results!=[(stages[1],b'2',b'0',b''),(stages[2],b'1',b'8',b'4109430a4409460a'),(stages[2],b'2',b'0',b'')]:
                raise ValueError(("octets du pipe",results))
            if b"A       C\nD       F\n" not in pipe.replace(b"\r\n",b"\n"):raise ValueError("rendu cut")
            record["essais"][-1].update(pid_producteur=int(stages[1]),pid_lecteur=int(stages[2]),stdout="4109430a4409460a",stderr="",statuts=[0,0])
            bad=b"cut: "+ "accès refusé\n".encode()
            streams("cut -f1,3 donnees.tsv",5,b"",bad)
            terminal.send(b"exit\n")
            terminal.expect(rb"absents=0x0 muets=0x0")
            if b"BLOC session terminee ok=true" not in terminal.data:
                raise ValueError("fermeture incomplète")
            record["succes"]=True
        except (OSError,EOFError,ValueError,TimeoutError) as error:
            record["erreur"]=str(error)
        finally:
            if vm.poll() is None: vm.terminate()
            vm.wait(10)
            log.write(vm.stdout.read())
    record.update(trace=digest(trace),p9_final=digest(disk),autorite_finale=digest(authority))
    (a.output/"resultat.json").write_text(json.dumps(record,indent=2,ensure_ascii=False)+"\n")
    print(json.dumps([a.arch,a.sector,record["succes"],record.get("erreur")]),flush=True)
    return record["succes"]

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("sector",type=int,choices=[512,4096])
    for k in ["iso","genesis","package","output"]:p.add_argument(k,type=Path)
    raise SystemExit(0 if run(p.parse_args()) else 1)
