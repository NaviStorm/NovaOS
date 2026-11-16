"""Deux ELF isolés sur un volume jetable et un deuxième fichier pour l'autorité."""
import argparse
import json
import os
from pathlib import Path
import re
import select
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts"))
from bloc_qemu import command

def qualify(arch, sector, iso, genesis, out, recovery=None, expected='ouvert', policy=None):
    out.mkdir(parents=True, exist_ok=False)
    disk, authority = out / "identites.p9", out / "autorite.raw"
    shutil.copyfile(genesis / "neuf-identites.p9", disk)
    shutil.copyfile(genesis / "neuf-autorite.raw", authority)
    if recovery:
        if expected != "recul":
            shutil.copyfile(recovery/"identites.p9",disk)
        shutil.copyfile(recovery/"autorite.raw",authority)
        if expected=="autorite":
            authority.write_bytes(bytes(12*4096))
    refusal={"initialise":b"NA3 domaine initialise enrolement refuse",
             "recul":b"NA3 registre divergent activation refusee",
             "autorite":b"NA3 autorite indisponible activation refusee"}.get(expected)
    if policy == "recul":
        refusal=b"NAP registre refuse avant lancement"
    completion=b"NA3 refus collecte" if refusal else b"NA3 quatre processus collectes"
    if policy:
        completion=refusal or b"NAP cinq processus collectes"
    cut={"avant":b"NAP coupure avant decision","apres":b"NAP coupure apres decision"}.get(policy)
    lifecycle=policy in ("mise-a-jour","reinstallation")
    if lifecycle: completion=b"NAP cycle installation termine"
    withdrawn = expected == "reponse-retiree"
    probed = False
    before=authority.read_bytes()
    assert (disk.stat().st_size, authority.stat().st_size) == (512*4096,12*4096)
    descriptor = out / "disque.vmdk"
    descriptor.write_text('# Disk DescriptorFile\nversion=1\nCID=abcd0033\nparentCID=ffffffff\ncreateType="twoGbMaxExtentFlat"\n' +
        ''.join(f'RW {f.stat().st_size//512} FLAT "{f.name}" 0\n' for f in (disk,authority)))
    argv = command(arch,sector,descriptor,controls=["-display","none","-monitor","none","-serial","stdio"])
    i=argv.index("-drive")+1
    argv[i]=argv[i].replace("format=raw","format=vmdk")
    env={**os.environ,"NOVA_ISO":str(iso.resolve()),"NOVA_ARCH":arch,"NOVA_ARM_CPU":"max","NOVA_VCPUS":"2"}
    data=bytearray()
    start=time.monotonic()
    stopping=False
    with subprocess.Popen(argv,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0) as vm:
        try:
            while vm.poll() is None and time.monotonic()-start<180:
                if not select.select([vm.stdout],[],[],1)[0]:continue
                chunk=os.read(vm.stdout.fileno(),8192)
                if not chunk:break
                data.extend(chunk)
                with (out/'invité.serial').open('ab') as trace:trace.write(chunk)
                if withdrawn and not probed and b"NA3 canal prive retire avant reponse" in data:
                    vm.stdin.write(b"ls /; echo NA3_PUBLIC_$?__fin\n")
                    vm.stdin.flush()
                    probed = True
                if withdrawn and (b"NA3_PUBLIC_0__fin" in data or re.search(rb"PROC pid=3 [^\r\n]*sortie=5", data)):
                    break
                if cut and cut in data:
                    vm.kill()
                    break
                if not stopping and completion in data:
                    vm.stdin.write(b"exit\n")
                    vm.stdin.flush()
                    stopping=True
                if b"absents=0x0 muets=0x0" in data or b"NA3 client echec" in data or re.search(rb"Panique du noyau[^\r\n]*\r?\n",data):break
        finally:
            if vm.poll() is None:vm.terminate()
            vm.wait(timeout=5)
            data.extend(vm.stdout.read())
            (out/"invité.serial").write_bytes(data)
    actors={}
    for role,marker in [("serveur","preuve acceptee"),("client","client preuve valide"),("provisionneur","provisionneur D attribue")]:
        match=re.search(rb'DEBUG pid=(\d+) NA3 '+marker.encode(),data)
        if match:actors[role]=int(match[1])
    ordinary=re.search(rb"DEBUG pid=(\d+) NA3 session et facteur revoques",data)
    if ordinary:actors["second_client"]=int(ordinary[1])
    ok=(len(set(actors.values()))==4 and all(x in data for x in (b"NA3 client termine",b"NA3 administrateur termine",b"NA3 client rejeu refuse",b"NA3 expiration refusee",b"NA3 signature incorrecte refusee",b"NA3 administration refusee",b"NA3 fermeture independante",b"NA3 trois sessions simultanees",b"NA3 D descendants retires temoin actif issue retrouvee",b"NA3 D autre titulaire refuse",b"NA3 D avant attribution refuse",b"NA3 reponse VFS manquante issue incertaine",b"NA3 desactivation confirmee",b"NA3 renomme et groupe retire",b"NA3 nouveau titulaire distinct",b"NA3 ancien facteur refuse apres recreation",b"NA3 quatre processus collectes",b"NA3 provisionneur D attribue",b"BLOC session terminee ok=true",b"absents=0x0 muets=0x0")))
    if refusal:
        ok=(refusal in data and b"NA3 refus collecte" in data and b"NA3 preuve acceptee" not in data and before==authority.read_bytes()
            and b"BLOC session terminee ok=true" in data and b"absents=0x0 muets=0x0" in data)
    if withdrawn:
        ok = probed and b"NA3_PUBLIC_0__fin" in data and not re.search(rb"PROC pid=3 [^\r\n]*sortie=[1-9]", data)
    if policy:
        required={
            "attribution":[b"NAP politique modifiee application arretee",b"NAP fermeture session reveil certain ecriture perdue reconciliee"],
            "retrait":[b"NAP retrait groupe descendants refuses lecture conservee"],
            "reprise":[b"NAP retrait persistant apres reauthentification"],
            "recul":[b"NAP registre refuse avant lancement"],
            "mise-a-jour":[b"NAP ELF mis a jour execute", b"NAP mise a jour attestee lecture autorisee",completion],
            "reinstallation":[b"NAP reinstallation meme ELF sans droits",completion],
            "avant":[cut], "apres":[cut],
        }[policy]
        if policy in ("attribution","retrait","reprise"):
            required.append(b"NAP zero handle preparation attente")
            required += [b"NAP deux titulaires authentifies",b"NAP meme image admise deux instances",b"NAP cinq processus collectes",b"NA3 preuve acceptee"]
        if cut:
            ok=vm.returncode==-9 and cut in data and b"NAP politique modifiee application arretee" not in data
        else:
            ok=all(marker in data for marker in required+[b"BLOC session terminee ok=true",b"absents=0x0 muets=0x0"])
        if lifecycle: ok=ok and b"NAP deux titulaires authentifies" in data
        if policy=="recul":
            ok=ok and before==authority.read_bytes() and b"NA3 preuve acceptee" not in data
        expected=policy
    (out/"resultats.json").write_text(json.dumps(dict(architecture=arch,secteur=sector,scenario=expected,
        processus=actors,seconde_hote=time.monotonic()-start,succes=ok),indent=2)+"\n")
    if not ok:raise RuntimeError(bytes(data[-1600:]))
if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("architecture",choices=("aarch64","x86_64"))
    p.add_argument("secteur",type=int,choices=(512,4096))
    for name in ("iso","genese","sortie"):p.add_argument(name,type=Path)
    p.add_argument("--apres",type=Path)
    p.add_argument("--attendu",choices=("ouvert","initialise","recul","autorite","reponse-retiree"),default="ouvert")
    a=p.parse_args()
    if bool(a.apres)!=(a.attendu in ("initialise", "recul", "autorite")):p.error("préciser ensemble le contrôle de reprise et sa source")
    qualify(a.architecture,a.secteur,a.iso,a.genese,a.sortie,a.apres,a.attendu)
