"""Exerce les ELF de livraison dans un checkout isolé du catalogue récepteur."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

p = argparse.ArgumentParser()
p.add_argument("recepteur", type=Path)
p.add_argument("arch", choices=["aarch64","x86_64"])
p.add_argument("sector", type=int, choices=[512,4096])
p.add_argument("out", type=Path)
p.add_argument("--rows",type=int,default=4096)
args = p.parse_args()
if args.rows < 64 or args.rows % 64: p.error("nombre positif multiple de 64 requis")
recv, out = args.recepteur.resolve(), args.out.resolve()
out.mkdir(parents=True,exist_ok=False)
sys.path.insert(0,str(recv/"novaOS/tests"))
from cli import Terminal
from pipe_natif import fixture
from journalisation import arret

os.environ.update(NOVA_VCPUS="2", NOVA_RAM="256", NOVA_ARCH=args.arch)
rows = [f"{(27*i)%64:02d}".encode().ljust(31,b"x")+b"\n" for i in range(args.rows)]
source = b"".join(rows)
expected = subprocess.check_output(["/opt/homebrew/bin/guniq", "-c"],
             input=subprocess.check_output(["/opt/homebrew/bin/gsort"], input=source,
                                           env={**os.environ,"LC_ALL":"C"}))
disk, _, _ = fixture(out, args.sector, {"donnees":source})
elf = recv/"livraison-uniq-0240/bin"/f"sort-{args.arch}.elf"
production = Path(__file__).resolve().parents[1]/"artifacts"/args.arch/"sort"
assert elf.read_bytes() == production.read_bytes()
report = {"recepteur": subprocess.check_output(["git","-C",recv,"rev-parse","HEAD"],text=True).strip(),
          "sort_sha256": hashlib.sha256(elf.read_bytes()).hexdigest(), "ram_mio":256,
          "secteur":args.sector, "entree_octets":len(source), "lignes":len(rows),
          "input_sha256":hashlib.sha256(source).hexdigest(), "groupes":64, "passages":[]}
for diag in [False,True]:
    mode = "diagnostic" if diag else "ordinaire"
    with Terminal(args.arch,args.sector,disk,mode,test=diag,journal=out/(mode+".serial")) as vm:
        vm.until(b"nsh> ")
        start=time.monotonic()
        cmd="sort donnees | uniq -c"
        vm.vm.stdin.write(f"echo DEBUT; {cmd}; echo ARCRET_$?__fin\n".encode())
        vm.until(b"\r\nDEBUT\r\n")
        raw=vm.until(b"__fin\r\n",timeout=240).replace(b"\r\n",b"\n")
        vm.until(b"nsh> ")
        match=re.search(rb"ARCRET_(\d+)__fin\n",raw)
        assert match,raw[-200:]
        body=raw[:match.start()]
        elapsed=time.monotonic()-start
        statuses={k.decode():int(v) for k,v in re.findall(rb"\[pipe (sort|uniq)=(\d+)\]",body)}
        # Les marqueurs de retrait noyau partagent l'UART métier sur le banc ordinaire.
        data=re.sub(rb"(?:DEBUG pid=\d+ TMP retrait confirme|TMP retrait instance=\d+ statut=0)\n",b"",body)
        ok=match[1]==b"0" and (statuses=={"sort":0,"uniq":0} if diag else data==expected)
        report["passages"].append(dict(mode=mode,commande=cmd,temps_pipeline_s=elapsed,
            statut=int(match[1]),statuts_enfants=statuses,sortie_hex=body.hex(),
            conforme=ok,sortie_gnu_sha256=hashlib.sha256(expected).hexdigest()))
        (out/"resultats.json").write_text(json.dumps(report,indent=2)+"\n")
        assert ok,(mode,raw[-300:])
        arret(vm,2)
