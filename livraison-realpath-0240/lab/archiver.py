"""Archiver une recette complète. Chaque dossier invité contient son resultats.json."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile

def digest(p):
    with p.open("rb") as f:
        return hashlib.file_digest(f,"sha256").hexdigest()

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("dossiers",type=Path,nargs=12)
    a=parser.parse_args()
    delivery=Path(__file__).resolve().parents[1]
    pieces={}
    synthesis={}
    table=["ISA / P9 | 1 chemin normalisé/s | 32 chemins via lien, chemins/s | 4 chemins profonds, chemins/s | Tas privé en fin de passage (Kio)",
           "---|---:|---:|---:|---:"]
    for directory in a.dossiers:
        record=json.loads((directory/"resultats.json").read_text())
        arch=record.get("arch") or record["architecture"]
        profile=f"{arch}/{record['secteur']}"
        profile_data=synthesis.setdefault(profile,{})
        if "elf" in record:
            kind="elf"
            assert record["elf"]==digest(delivery/f"bin/realpath-{arch}.elf")
        elif "medianes" in record:
            kind="moteur"
            rates=[record["medianes"][str(n)]["chemins_s"] for n in [1,32,4]]
            heap=max(r["tas_octets"] for r in record["mesures"])/1024
            table.append(f"{profile} | {rates[0]:.0f} | {rates[1]:.0f} | {rates[2]:.0f} | {heap:g}")
        else:
            kind="nsh"
            assert record["avant"]==record["apres"]
        assert kind not in profile_data
        profile_data[kind]=record
        for path in directory.iterdir():
            if path.is_file() and not path.name.startswith("."):
                pieces[f"qemu/{profile}/{kind}/{path.name}"]=path
    assert set(synthesis)=={f"{a}/{s}" for a in ("aarch64","x86_64") for s in (512,4096)}
    for record in synthesis.values():
        assert set(record)=={"moteur","nsh","elf"}
        assert len(record["moteur"]["cas"])==57 and len(record["elf"]["cas"])==8
        assert len(record["nsh"]["commandes"])==15
    (delivery/"preuves/qemu.json").write_text(json.dumps(synthesis,indent=1,ensure_ascii=False)+"\n")
    (delivery/"preuves/tableau-qemu.md").write_text("\n".join(table)+"\n")
    for folder in ("bin","preuves"):
        for path in (delivery/folder).rglob("*"):
            if path.is_file() and not path.name.startswith("."):
                pieces[str(path.relative_to(delivery))]=path
    with tarfile.open(delivery/"qualification.tar.xz","w:xz",preset=3) as tar:
        for name,path in sorted(pieces.items()):tar.add(path,arcname=name,recursive=False)
    (delivery/"qualification.sha256").write_text("".join(f"{digest(p)}  {name}\n" for name,p in sorted(pieces.items())))
    subprocess.run(["python3","-B",delivery/"lab/verifier.py"],check=True)
    files=[p for p in delivery.rglob("*") if p.is_file() and p.name!="SHA256SUMS"
           and not any(x.startswith(".") or x=="__pycache__" for x in p.relative_to(delivery).parts)]
    (delivery/"SHA256SUMS").write_text("".join(f"{digest(p)}  {p.relative_to(delivery)}\n" for p in sorted(files)))

if __name__=="__main__":
    main()
