"""Mesurer stdout, stderr et la sortie du PID cmp sélectionné dans une image de recette."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "novaOS/tests"))
from cli import HOST, Terminal
from journalisation import arret
from pipe_natif import host
from preuves_stockage import empreinte, figer_initiale

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("architecture", choices=["aarch64", "x86_64"])
p.add_argument("secteur", choices=[512, 4096], type=int)
p.add_argument("noyau", type=Path, help="noyau construit avec admission-recette.patch.gz")
p.add_argument("sortie", type=Path, help="répertoire neuf")
p.add_argument("--mesures", action="store_true", help="chronométrer les fichiers de recette P9")
a = p.parse_args()
sector = str(a.secteur)
out = a.sortie.resolve()
out.mkdir(parents=True, exist_ok=False)
os.environ.update(NOVA_KERNEL_ELF=str(a.noyau.resolve()), NOVA_RAM="256", NOVA_VCPUS="2")
source, disk = out/"genese.raw", out/"session.raw"
small = {"a": b"x\n\0\x80\xff", "b": b"x\n\x01\x7f\xfe", "vide": b"", "lf": b"x\n",
         "partiel": b"x\n\0", "copie": b"x\n\0\x80\xff", "-l": b"x\n\0\x80\xff",
         "k1": bytes(1000)+b"x"+bytes(23)+b"X", "k2": bytes(1024)+b"Y"}
small.update({"a.bin":b"\0\n\xff","b.bin":b"\0\n\xfe"})
commands = []
for name, data in small.items():
    commands.append("create /"+name)
    for i in range(0,len(data),512):
        commands.append(f"write /{name} {i} {data[i:i+512].hex()}")
for name in ("grand1", "grand2"):
    commands.extend(["create /"+name, f"write /{name} 262143 00"])
commands.append("write /grand2 0 58")
if a.mesures:
    for size in [1<<20,100<<20,(256<<20)+1,1<<30]:
        for label in ("a","b","debut","milieu","fin","court","liste") if size==1<<20 else ("a","b","debut"):
            name = f"/{label}{size}"
            commands.extend(["create "+name,f"write {name} {size-1-int(label=='court')} 00"])
            for pos in {"debut":[0],"milieu":[size//2],"fin":[size-1],
                        "liste":[0,size//2,size-1]}.get(label,[]):
                commands.append(f"write {name} {pos} 58")
with source.open("xb") as dest:
    gen = subprocess.run([HOST, "script", "512", sector], stdout=dest, stderr=subprocess.PIPE,
                         input=("\n".join(commands)+"\n").encode(), check=True)
fs, lv = re.search(rb"FS=(\w+) LV=(\w+)", gen.stderr).groups()
with disk.open("xb") as dest:
    host(source, "migrate-content-p9", "512", sector, fs, lv, output=dest)
figer_initiale(disk)

def corpus():
    slots = re.findall(r"slot=(\d+) génération=(\d+)", host(disk,"inspect","512",sector).decode())
    slot = max(slots,key=lambda s:int(s[1]))[0]
    hashes = {}
    for name in ["grand1","grand2"]:
        value = host(disk,"salvage","512",sector,fs,lv,slot,"/"+name)
        assert value == (b"X" if name=="grand2" else b"\0") + bytes(262143)
        hashes[name] = hashlib.sha256(value).hexdigest()
    return hashes

before = corpus()
observations = []
serial = out/"session.serial"
def child(vm, args, key, expected):
    cmd = shlex.join(["cmp",*args])
    start = time.monotonic_ns()
    vm.vm.stdin.write((cmd+"\n").encode())
    trace = vm.until(b"CMP WAIT fault=", timeout=600) + vm.until(b"nsh> ")
    elapsed = (time.monotonic_ns()-start)/1e9
    pid = re.search(rb"CLI spawn image=240 pid=(\d+)", trace)
    assert pid, (cmd,trace[-900:])
    pid = pid[1]
    status = re.search(rb"PROC pid="+pid+rb" niveau=\S+ pc=\S+ sortie=(\d+)", trace)
    assert status and b"CLI collecte pid="+pid in trace, (cmd,trace[-1000:])
    rc = int(status[1])
    assert f"CMP WAIT fault=false code={rc}".encode() in trace
    streams = {2:bytearray(),4:bytearray()}
    for port, part in re.findall(rb"CMP RAW pid="+pid+rb" port=(\d+) octets=\[([0-9a-f, ]*)\]", trace):
        streams[int(port)].extend(bytes.fromhex(part.decode().replace(",","")))
    measured = [rc,streams[2].hex(),streams[4].hex()]
    assert measured == expected, (key,expected,measured,trace[-1800:])
    observations.append(dict(id=key,commande=cmd,pid=int(pid),mesure=measured,secondes_nsh=elapsed))

with Terminal(a.architecture,a.secteur,disk,"cmp-statut",test=True,journal=serial) as vm:
    vm.until(b"nsh> ")
    child(vm,["-l","grand1","grand2"],"verbose-ecart-premier-bloc",
          [1,b"     1   0 130\n".hex(),""])
    child(vm,["a.bin","b.bin"],"deux-fichiers-delegues",
          [1,b"a.bin b.bin differ: char 3, line 2\n".hex(),""])
    if a.mesures:
        for n in [1<<20,100<<20,(256<<20)+1,1<<30]:
            left = f"a{n}"
            for label,flags,right,diffs,count in [
                ("début",[],"debut",[1],0),("même objet",[],"a",[],0),
                ("limite",["-n","65536"],"b",[],65536),
                ("deux sauts",["-i",f"{n-65536}:{n-65535}","-n65535"],"b",[],65535),
                ("égaux",[],"b",[],n),("milieu",[],"milieu",[n//2+1],0),
                ("fin",[],"fin",[n],n),("EOF",[],"court",[],n-1),
                ("liste",["-l"],"liste",[1,n//2+1,n],n)]:
                if n!=1<<20 and label in ("égaux","milieu","fin","EOF","liste"):
                    continue
                other = f"{right}{n}"
                stdout = stderr = b""
                if label=="liste":
                    stdout = "".join(f"{pos:{len(str(n))}}   0 130\n" for pos in diffs).encode()
                elif diffs:
                    stdout = f"{left} {other} differ: char {diffs[0]}, line 1\n".encode()
                elif label=="EOF":
                    stderr = f"cmp: EOF on '{other}' after byte {n-1}, in line 1\n".encode()
                child(vm,[*flags,left,other],f"mesure-{n}-{label}",
                      [int(bool(diffs) or label=="EOF"),stdout.hex(),stderr.hex()])
                observations[-1].update(taille=n,traitement=label,parcours_pour_debit=count)
    rows = json.loads((HERE/"preuves/options.json").read_text())["cas"]
    for row in ([] if a.mesures else rows):
        args = row["args"]
        # Deux entrées régulières présentes, sans stdin. Les erreurs d'option sont traitées par nsh.
        if row["document"] or row["mesures"]["Nova"][0] > 1 or row["locale"] != "C":
            continue
        if len([v for v in args if v in small]) < 2 or any(v in ["-","grand1","grand2"] for v in args):
            continue
        child(vm,args,row["id"],row["mesures"]["Nova"])
    arret(vm,2)
assert corpus()==before
iso = serial.with_suffix(".iso")
kernel = out/"noyau.elf"
subprocess.run(["xorriso","-osirrox","on","-indev",iso,"-extract","/boot/novaos.elf",kernel],
               check=True,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
elf = HERE/"bin"/("cmp-"+a.architecture+".elf")
assert empreinte(kernel)==empreinte(a.noyau) and elf.read_bytes() in kernel.read_bytes()
(out/"resultats.json").write_text(json.dumps(dict(iso=empreinte(iso),noyau=empreinte(kernel),
    elf=empreinte(elf),fichiers=before,octets_par_fichier=262144,observations=observations),indent=2)+"\n")
