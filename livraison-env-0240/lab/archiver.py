"""Archiver deux recettes QEMU terminées et vérifier tous les membres décompressés."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import statistics
import tarfile
from options import sha

def archive_members(dest,name_prefix,sources):
    archive=dest/(name_prefix+".tar.gz")
    manifest={name:sha(path) for name,path in sources.items()}
    listing="".join(f"{h}  {n}\n" for n,h in sorted(manifest.items())).encode()
    with tarfile.open(archive,"w:gz",compresslevel=6) as t:
        for name,path in sources.items():t.add(path,arcname=name,recursive=False)
        info=tarfile.TarInfo("SHA256SUMS");info.size=len(listing)
        t.addfile(info,io.BytesIO(listing))
    with tarfile.open(archive,"r:gz") as t:
        assert sorted(t.getnames())==sorted([*manifest,"SHA256SUMS"])
        assert t.extractfile("SHA256SUMS").read()==listing
        for name,digest in manifest.items():
            with t.extractfile(name) as f:assert hashlib.file_digest(f,"sha256").hexdigest()==digest
    (dest/(name_prefix+"-membres.sha256")).write_bytes(listing)
    print(f"{len(manifest)} membres relus dans {archive.name}. SHA-256 {sha(archive)}")

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("aarch64",type=Path)
    p.add_argument("x86_64",type=Path)
    p.add_argument("destination",type=Path)
    a=p.parse_args()
    dest=a.destination
    dest.mkdir(parents=True,exist_ok=True)
    sources={}
    matrix=[]
    for arch,root in [("aarch64",a.aarch64),("x86_64",a.x86_64)]:
        vectors=json.loads((root/"vecteurs.json").read_text())
        for sector in [512,4096]:
            cell=root/str(sector)
            row=json.loads((cell/"resultats.json").read_text())
            assert (row["arch"],row["secteur"])==(arch,sector)
            for profile in ["sdk","temoin","nsh"]:
                assert sha(cell/(profile+".iso"))==row[profile]["iso"]
            for profile in ["sdk","temoin"]:
                assert row[profile]["image_avant"]==row[profile]["image_apres"]
            checks=row["sdk"]["commandes"]
            assert len(checks)==len(vectors)
            for case,check in zip(vectors,checks,strict=True):
                assert case["statut"]==check["statut"] and case["sortie"]==check["stdout"]+check["stderr"]
            row["arguments_sdk"]=[v["argv"] for v in vectors]
            matrix.append(row)
        for path in sorted(root.rglob("*")):
            if path.is_file():
                sources[f"{arch}/{path.relative_to(root)}"]=path
    archive_members(dest,"qemu",sources)
    (dest/"matrice-cibles.json").write_text(json.dumps(matrix,ensure_ascii=False,indent=1)+"\n")
    lines=["| ISA | Secteur P9 | Affectations | Octets sur stdout | Temps ELF (ms) | Préparation incluse (ms) | Pages initiales données + pile |",
           "|---|---:|---:|---:|---:|---:|---:|"]
    for row in matrix:
        for n in [1,30]:
            checks=[x for x in row["sdk"]["commandes"] if x["test"].startswith(f"mesure-{n}-")]
            med=row["sdk"]["mediane"][str(n)]
            assert len(checks)>=4
            x=checks[-1]
            lines.append(f"| {row['arch']} | {row['secteur']} | {n} | {len(bytes.fromhex(x['stdout']))} | {med['compteur_ms']:.3f} | {med['preparation_ms']:.3f} | {x['pages_donnees_initiales']} + {x['pages_pile_initiales']} |")
    (dest/"performances-qemu.md").write_text("\n".join(lines)+"\n")
    host_names=["options.json","performances.json","performances.md","references.json","amont.json",
                "amont-GNU.log","amont-Nova.log","catalogue.txt","catalogue-rosetta.txt","registre-abi.log"]
    if (dest/"options.json").exists():
        archive_members(dest,"hote",{n:dest/n for n in host_names})
if __name__=="__main__":main()
