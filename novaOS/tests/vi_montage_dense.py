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
    a=p.parse_args()
    with tempfile.TemporaryDirectory(prefix="nova-vi-dense-") as tmp:
        image=Path(tmp)/"volume.raw"
        if subprocess.run(["cp","-c",a.image,image]).returncode:
            shutil.copyfile(a.image,image)
        with Terminal(a.arch,4096,image,"vi-dense-"+str(a.mio),test=True) as t:
            try:
                t.until(b"nsh> ",900,refus=b"BLOC session terminee ok=false")
            except RuntimeError as e:
                data=t.archive.read_bytes()
                if b"BLOC session terminee ok=false" not in data or b"BLOC bilan mode=42" not in data or b"client=Exit(8)" not in data:
                    raise
                result=dict(montage=False,statut_client=8,vi_lance=b"CLI spawn image=14" in data)
                assert not result["vi_lance"]
            else:
                n=a.mio<<20
                name=f"f{n}"
                start=time.monotonic_ns()
                t.vm.stdin.write(f"vi {name}\n".encode())
                t.until(b"\x1b[6n",600)
                t.vm.stdin.write(b"\x1b[24;80R")
                view=t.until(f"{name} [enregistré] 1:1".encode(),600)
                assert b"A"*64 in view
                result=dict(montage=True,vi_lance=True,premier_ecran_ms=round((time.monotonic_ns()-start)/1e6,3))
                t.vm.stdin.write(b":q\r")
                t.until(b"nsh> ")
                t.exit()
    a.rapport.write_text(json.dumps(dict(architecture=a.arch,octets_denses=a.mio<<20,**result),indent=2)+"\n")
