"""Exécute un parcours dans un seul invité. Chaque départ utilise une copie P9."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
from paquet_qemu import Serial, digest, prepare_vm

RAW = b"A\tC\nD\tF\n"
ARGV = b"\0\0\3\0a b\2\0-x"

def composants(iso):
    placements=json.loads(iso.with_suffix(".composants.json").read_text())
    result={}
    with iso.open("rb") as image:
        for name,ref in placements.items():
            image.seek(ref["offset"])
            raw=image.read(ref["taille"])
            observed=hashlib.sha256(raw).hexdigest()
            if len(raw)!=ref["taille"] or observed!=ref["sha256"]:
                raise ValueError(("image de démarrage différente",name))
            result[name]=observed
    return result

def identites(log):
    storage=re.search(rb"BLOC chaine novaFS=(\d+) bloc=(\d+) pilote=(\d+) client=(\d+)",log)
    login=re.search(rb"NXLOGIN route shell=(\d+) verificateur=(\d+) signataire=(\d+)",log)
    relay=re.search(rb"NXRELAY branche shell=(\d+) verificateur=(\d+) courtier=(\d+)",log)
    if not all((storage,login,relay)) or storage[4]!=login[1] or login[1]!=relay[1] or login[2]!=relay[2]:
        raise ValueError("routes de démarrage")
    return {key:int(pid) for key,pid in zip(("P9","bloc","pilote","nsh","NovaAuth","signataire","courtier"),(*storage.groups(),login[2],login[3],relay[3]))}

def run(a):
    argv, env, disk, authority = prepare_vm(a.arch,a.sector,a.iso,a.genesis,a.output,"")
    record = dict(architecture=a.arch, secteur=a.sector, parcours=a.group, iso=digest(a.iso),
                  p9_initial=digest(disk), autorite_initiale=digest(authority), tests=[], succes=False)
    log=a.output/"invite.serial"
    with log.open("xb") as trace, subprocess.Popen(argv,env=env,stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0) as vm:
        term=Serial(vm,trace)
        def evaluate(line,data=None):
            start=term.offset
            # Lire $? dans la même liste, avant toute autre commande du banc.
            term.submit(line+"; echo NXR_$?__FIN",data)
            status=term.expect(rb"NXR_(\d+)__FIN\r?\n")
            term.expect(rb"nsh> ")
            return int(status[1]),bytes(term.data[start:term.offset])
        def command(line,rc=None,data=None):
            actual, out=evaluate(line,data)
            record["tests"].append(dict(commande=line,statut=actual,attendu=rc))
            if rc is not None and actual!=rc:raise ValueError((line,actual,rc))
            return out
        def streams(line, rc, expected, *, pipe=False, reader_refuses_file=False):
            out=command(line,rc)
            observed=re.findall(rb"NXFLUX pid=(\d+) fd=([12]) octets=(\d+) prefixe=([0-9a-f]*)",out)
            by_pid={}
            for p,f,n,h in observed:
                if int(n)>64 or len(bytes.fromhex(h.decode()))!=int(n):raise ValueError(("capture",line))
                by_pid.setdefault(int(p),{})[int(f)]=bytes.fromhex(h.decode())
            if pipe:
                pair=re.search(rb"NXPIPE producteur=(\d+) statut=(\d+) lecteur=(\d+) statut=(\d+)",out)
                if not pair:raise ValueError(("collecte",line))
                p,w,r,v=map(int,pair.groups())
                # Si le lecteur refuse son fichier avant que cat ait écrit,
                # le producteur reçoit CLOSED. Les lecteurs nominaux doivent tout lire.
                producer_errors = {0: b""}
                if reader_refuses_file:
                    producer_errors[11] = "capacité révoquée".encode()
                if p==r or v!=rc or w not in producer_errors or by_pid.get(p)!={2:producer_errors[w]}:
                    raise ValueError(("statuts",line,(p,w,r,v),by_pid))
                record["tests"][-1]["processus"] = {"producteur":p,"statut_producteur":w,
                                                    "lecteur":r,"statut_lecteur":v}
            elif len(by_pid)==1:r=next(iter(by_pid))
            else:raise ValueError(("PID",line,observed))
            if by_pid.get(r)!={1:expected[0],2:expected[1]}:raise ValueError(("octets",line,by_pid))
            record["tests"][-1]["flux"]={p:{f:v.hex() for f,v in streams.items()} for p,streams in by_pid.items()}
            return out
        try:
            term.expect(rb"nsh> ")
            if a.group in ("chaud","chaud-baux"):
                record["empreintes_avant"]=composants(a.iso)
                record["processus_systeme"]=identites(term.data)
                marker=term.offset
                command("temoin",127)
                blob=a.package.read_bytes()
                command("install --hex "+str(len(blob)),0,blob)
                if a.group == "chaud-baux":
                    streams("temoin",0,(b"A\tB\tC\nD\tE\tF\n",b""))
                    record["tests"][-1]["bail"] = "initial"
                    term.submit("session reauthentifier")
                    term.expect("NovaAuth : nouvelle session authentifiée".encode())
                    term.expect(rb"nsh> ")
                    record["reauthentification_explicite"] = True
                streams("temoin | cut -f1,3",0,(RAW,b""),pipe=True)
                streams("/system/bin/temoin",0,(b"A\tB\tC\nD\tE\tF\n",b""))
                if a.group == "chaud":
                    streams("temoin",0,(b"A\tB\tC\nD\tE\tF\n",b""))
                else:
                    for test in record["tests"][-2:]:
                        test["bail"] = "nouveau"
                record["empreintes_apres"]=composants(a.iso)
                if record["empreintes_avant"]!=record["empreintes_apres"] or digest(a.iso)!=record["iso"]:
                    raise ValueError("composants modifiés en cours de session")
                tail=term.data[marker:]
                if b"NXLOGIN route" in tail or b"NXRELAY branche" in tail or any(
                    re.search(fr"PROC pid={pid}\b.*sortie=".encode(),tail) for pid in record["processus_systeme"].values()):
                    raise ValueError("redémarrage d'un composant")
            elif a.group=="cut":
                command("cd /travail",0)
                streams("cut -f1,3 donnees.tsv",0,(RAW,b""))
                streams("/system/bin/cut -f1,3 donnees.tsv",0,(RAW,b""))
                command("cd /system/bin",0)
                streams("./cut -f1,3 /travail/donnees.tsv",0,(RAW,b""))
            elif a.group=="pipe":
                command("cd /travail",0)
                streams("cat donnees.tsv | cut -f1,3",0,(RAW,b""),pipe=True)
                streams("cat donnees.tsv | cut -f1,3 donnees.tsv",5,(b"","cut: accès refusé\n".encode()),pipe=True,reader_refuses_file=True)
            elif a.group=="argv":
                streams("nsh /travail/arguments.nsh",0,(ARGV,b""))
                streams("""nsh -c "temoin '' 'a b' -x" """.strip(),0,(ARGV,b""))
            elif a.group=="redirect":
                command("cd /travail",0)
                streams("cut -f1,3 <donnees.tsv >sortie 2>erreurs",0,(RAW,b""))
                streams("cut -f1,3 <sortie",0,(b"A\nD\n",b""))
                for line in ("cut -f1,3 <donnees.tsv >sortie","cut -f1,3 <donnees.tsv 2>erreurs"):
                    status,out=evaluate(line)
                    record["tests"].append(dict(commande=line,statut=status,diagnostic="existe"))
                    if not status or "exist" not in out.decode(errors="replace"):raise ValueError(("existant",line,status))
                command("cut -f1,3 </system/identity/users.db",5)
                command("cut -f1,3 <donnees.tsv >/system/interdit",5)
            elif a.group=="auteurs":
                command("cd /travail",0)
                for mode,line in [("direct","cut -f1,3 donnees.tsv"),("pipe","cat donnees.tsv | cut -f1,3")]:
                    out=streams(line,0,(RAW,b""),pipe=(mode=="pipe"))
                    targets=re.findall(rb"NXVFS ELF pid=(\d+) objet=(\d+) ouverture=(\d+)",out)
                    expected=re.search(rb"NXPIPE producteur=(\d+)" if mode=="pipe" else rb"NXFLUX pid=(\d+)",out)
                    if len(targets)!=1 or targets[0][0]!=expected[1] or b"NXVFS volume principal ouverture TSV" in out:
                        raise ValueError(("auteur de l'ouverture",line,targets))
                    record["tests"][-1]["ouverture_tsv"]={"pid":int(targets[0][0]),"objet":int(targets[0][1])}
            elif a.group=="redirpipe":
                command("cd /travail",0)
                streams("cat - <donnees.tsv | cut -f1,3 >paire 2>err-paire",0,(RAW,b""),pipe=True)
                streams("cut -f1,3 absent 2>diagnostic",5,(b"","cut: accès refusé\n".encode()))
            elif a.group == "selection":
                command("cd /travail", 0)
                for line, expected, writer in (
                    ("cat donnees.tsv | wc -l", b"2\n", 0),
                    ("cat donnees.tsv | head -n 1", b"A\tB\tC\n", 0),
                    ("cat lignes.tsv | head -n 1", b"A\tB\tC\n", 11),
                ):
                    out = command(line, 0)
                    live = re.search(rb"NXVPIPE NovaAuth reponse tag=(\d+)", out)
                    statuses = re.search(rb"NXVPIPE producteur=(\d+) statut=(\d+) lecteur=(\d+) statut=(\d+)", out)
                    if not live or not statuses or live.end() > statuses.start():
                        raise ValueError(("réponse NovaAuth ou collecte absente", line))
                    p, w, r, v = map(int, statuses.groups())
                    if p == r or w != writer or v != 0:
                        raise ValueError(("résultats enfants", line, (p, w, r, v)))
                    frames = re.findall(rb"NXFLUX pid=(\d+) fd=([12]) octets=(\d+) prefixe=([0-9a-f]*)",out)
                    expected_frames = [(r, 1, len(expected), expected.hex()), (r, 2, 0, "")]
                    if [(int(i), int(fd), int(sz), raw.decode()) for i, fd, sz, raw in frames] != expected_frames:
                        raise ValueError(("octets bruts", line, frames))
                    record["tests"][-1].update(producteur=p, lecteur=r, statut_producteur=w,
                            statut_lecteur=v, reponse_auth=int(live[1]), stdout=expected.hex(), stderr="")
            elif a.group == "selection-refus":
                command("cd /travail", 0)
                for mode in ("quota", "collision", "partiel", "expire", "revoque"):
                    out = command("raccord-test " + mode, 0)
                    if ("NXVPIPE temoin " + mode + " termine").encode() not in out:
                        raise ValueError(("sonde absente", mode))
                    if mode == "quota":
                        refused = re.findall(rb"BUDGET create pid=(\d+) roots=4 err=6 before=(\([^)]*\)) after=(\([^)]*\))",out)
                        if len(refused) != 1 or refused[0][1] != refused[0][2]:
                            raise ValueError(("cinquième racine", refused))
                        record["tests"][-1]["compteurs_refus"] = refused[0][1].decode()
                command("true", 0)
            elif a.group=="historiques":
                command("cd /travail",0)
                wc=command("cat donnees.tsv | wc -l",0).replace(b"\r\n",b"\n")
                head=command("cat donnees.tsv | head -n 1",0)
                record["sorties_historiques"] = {
                    "wc_deux_lignes": b"\n2\n" in wc,
                    "head_premiere_ligne": bool(re.search(rb"A[ \t]+B[ \t]+C",head)),
                }
                command("false",1)
                command("false --help",0)
                command("exit --help",0)
                command("true",0)
            elif a.group=="syntaxe":
                command("cd /travail",0)
                for bad in (">>", "2>&1", "3>", "1>", "<<", ">|"):
                    line=f"cut -f1,3 {bad} interdit"
                    got,out=term.command(line)
                    if not got:raise ValueError(("opérateur accepté",line))
                    record["tests"].append(dict(commande=line,statut=got))
                # Une commande interne conserve sa priorité, sans créer la cible.
                command("cd / >interdit",3)
                command("exit >interdit",3)
                literal=command(r"echo 'a>b' \<x",0)
                if b"a>b <x" not in literal: raise ValueError("opérateur protégé réinterprété")
            elif a.group=="binaire":
                command("cd /travail",0)
                streams("""temoin '' 'a b' -x >octets""",0,(ARGV,b""))
            elif a.group=="droits":
                command("cd /travail",0)
                command("redir-test concurrence course",0)
                command("redir-test retrait donnees.tsv revoque",0)
            elif a.group=="diagnostic":
                for line in a.command:command(line)
            term.send(b"exit\n")
            term.expect(rb"absents=0x0 muets=0x0")
            if b"BLOC session terminee ok=true" not in term.data:raise ValueError("clôture stockage")
            if not all(record.get("sorties_historiques", {}).values()):
                raise ValueError("sorties des pipes historiques")
            record["succes"]=True
        except (ValueError,TimeoutError,EOFError,OSError) as err:
            record["erreur"]=str(err)
        finally:
            if vm.poll() is None:vm.terminate()
            vm.wait(10)
            trace.write(vm.stdout.read())
    record.update(serial=digest(log),p9_final=digest(disk),autorite_finale=digest(authority))
    (a.output/"resultat.json").write_text(json.dumps(record,indent=2,ensure_ascii=False)+"\n")
    print(a.arch,a.sector,a.group,record["succes"],record.get("erreur",""),flush=True)
    return record["succes"]

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("sector",type=int,choices=[512,4096])
    for name in ["iso","genesis","package","output"]:p.add_argument(name,type=Path)
    p.add_argument("group",choices=["chaud","chaud-baux","cut","pipe","argv","redirect","redirpipe","droits","binaire","auteurs","syntaxe","historiques","diagnostic","selection","selection-refus"])
    p.add_argument("command",nargs="*")
    raise SystemExit(0 if run(p.parse_args()) else 1)
