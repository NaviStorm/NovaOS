"""Écho ARM du gestionnaire commun 4E.1, sur le même vecteur de 1 514 octets que Q35."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

from qualifier import BASE, PORT, output, sha
from gestionnaire_4d.qualifier import build

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive",type=lambda v: Path(v).resolve())
    p.add_argument("--compiler",action="store_true")
    p.add_argument("--revision",default="HEAD")
    a=p.parse_args()
    if output("git","status","--porcelain","--","novaOS/kernel","novaOS/user","novaOS/libs"):
        raise ValueError("Sources modifiées")
    rev=output("git","rev-parse","--verify",a.revision+"^{commit}")
    exe=a.archive/"images/ethernet-4f/arm.elf"
    if not exe.exists() and rev!=output("git","rev-parse","HEAD"):
        raise ValueError("Image ARM archivée absente")
    exe,rev=build("echo1514-reset",a.archive,reprise="ethernet-4f",revision=rev)
    if a.compiler: return
    outils=BASE/"archives-ethernet-phase3/reproduction-8bd3f9c"
    epoques=set()
    for n in range(1,4):
        dest=a.archive/"ethernet"/str(n)
        if not (dest/"campagne.json").exists():
            subprocess.run([sys.executable,"-B",PORT,"arm","echo1514-reset",exe,dest,
                BASE/"pilote-virtio-net",BASE/"archives-virtio-net",outils/"limine-enroll",outils/"BOOTAA64.EFI",
                "--limine-arm",outils/"limine-a72.EFI","--source-elf",rev,"--cycle-vie","--esp-verifiee",
                "--gestionnaire-4e1","ethernet-4f","--delai","300"],check=True)
        c=json.loads((dest/"campagne.json").read_text())
        serie=(dest/"serie.log").read_text()
        boot,=re.findall(r"E41 demarrage boot=([0-9a-f]{32})",serie)
        if not (c["succes"] and c["elf_relu_esp"]==c["elf"]==sha(exe) and c["volume_p9"] and
                c["trames_par_sens"]==13312 and "creations=1 epingles=0 mappes=0" in serie):
            raise ValueError("Régression de l’écho ARM ou de sa restitution")
        epoques.add(boot)
    if len(epoques)!=3: raise ValueError("Démarrages ARM distincts attendus")

if __name__=="__main__":
    main()
