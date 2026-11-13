"""Essai de bout en bout sur une COPIE jetable d'une image P9 dense déjà vérifiée."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from cli import Terminal

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("image",type=Path)
    p.add_argument("mio",type=int,choices=[16,100,1024])
    p.add_argument("rapport",type=Path)
    p.add_argument("--secteur", type=int, choices=[512,4096], default=4096)
    p.add_argument("--parcourir", action="store_true")
    p.add_argument("--observation", type=int, default=3600,
                   help="limite murale du banc hôte pour observer le montage, en secondes")
    p.add_argument("--sondes",action="store_true",help="jalons du service, variante de laboratoire")
    a=p.parse_args()
    if a.observation < 1:
        p.error("durée positive requise")
    with tempfile.TemporaryDirectory(prefix="nova-vi-dense-") as tmp:
        image=Path(tmp)/"volume.raw"
        if subprocess.run(["cp","-c",a.image,image]).returncode:
            shutil.copyfile(a.image,image)
        with Terminal(a.arch,a.secteur,image,"vi-dense-"+str(a.mio),test=True) as t:
            debut=time.monotonic_ns()
            montage_ms=None
            try:
                if a.sondes:
                    t.until(b"IO-MONTAGE debut",60)
                    debut_service=time.monotonic_ns()
                    t.until(b"IO-MONTAGE fin",a.observation)
                    montage_ms=round((time.monotonic_ns()-debut_service)/1e6,3)
                t.until(b"nsh> ",a.observation,refus=b"BLOC session terminee ok=false")
            except RuntimeError as e:
                data=t.archive.read_bytes()
                if time.monotonic_ns()-debut >= a.observation*1_000_000_000 and b"BLOC session terminee" not in data and b"Panique du noyau" not in data:
                    return_observation=dict(architecture=a.arch, secteur=a.secteur, octets_denses=a.mio<<20,
                        observation_s=a.observation, issue="limite du banc hôte", vi_lance=False,
                        lots_bloc_32=data.count(b"BLOC parcours i="))
                    a.rapport.write_text(json.dumps(return_observation,indent=2)+"\n")
                    raise SystemExit(0)
                if b"BLOC session terminee ok=false" not in data or b"BLOC bilan mode=42" not in data or b"client=Exit(8)" not in data:
                    raise
                result=dict(montage=False,statut_client=8,vi_lance=b"CLI spawn image=14" in data)
                assert not result["vi_lance"]
            else:
                boot=(time.monotonic_ns()-debut)/1e6
                n=a.mio<<20
                name=f"f{n}"
                start=time.monotonic_ns()
                t.vm.stdin.write(f"vi {name}\n".encode())
                t.until(b"\x1b[6n",600)
                t.vm.stdin.write(b"\x1b[24;80R")
                view=t.until(f"{name} [enregistré] 1:1".encode(),600)
                assert b"A"*64 in view
                result=dict(montage=True,vi_lance=True,premier_ecran_ms=round((time.monotonic_ns()-start)/1e6,3))
                result["demarrage_jusqu_au_shell_ms"]=round(boot,3)
                if a.sondes: result["montage_service_ms"]=montage_ms
                if a.mio in (100,1024):
                    # Entrer réellement dans la fenêtre suivante (32 lignes de 128 octets).
                    start=time.monotonic_ns()
                    t.vm.stdin.write(b"j"*40)
                    t.until(f"{name} [enregistré] 41:1".encode(),180)
                    result["navigation_40_lignes_ms"]=round((time.monotonic_ns()-start)/1e6,3)
                if a.parcourir:
                    start=time.monotonic_ns()
                    t.vm.stdin.write(b"G")
                    t.until(f"{name} [enregistré] {n//128+1}:1".encode(),3600)
                    result["navigation_fin_ms"]=round((time.monotonic_ns()-start)/1e6,3)
                t.vm.stdin.write(b":q\r")
                t.until(b"nsh> ")
                t.exit()
    a.rapport.write_text(json.dumps(dict(architecture=a.arch,secteur=a.secteur,octets_denses=a.mio<<20,**result),indent=2)+"\n")
