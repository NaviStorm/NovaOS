"""Rassembler les quatre rapports QEMU et vérifier les comptes avec le Mac."""
import gzip
import json
from pathlib import Path
import sys

def read(path): return json.loads(path.read_text())

def main():
    root=Path(__file__).resolve().parents[1]
    proof=root/"preuves"
    host=read(proof/"mesures-mac.json")["rows"]
    detailed=[]
    coverage={}
    lines=["Profil          | Taille         | Traitement      | Nova (s) | Mio/s Nova | Octets produits"]
    expected_cases=sum(1 for _ in gzip.open(proof/"differentiel.jsonl.gz"))
    for arch in ["aarch64","x86_64"]:
        for sector in [512,4096]:
            folder=Path(sys.argv[1])/f"tr-livraison-{arch}-{sector}"
            doc=read(folder/"nsh/resultats.json")
            assert doc["avant"]==doc["apres"] and doc["ram"]==256*2**20
            key=f"{arch}/{sector}"
            coverage[key]=dict(moteur_sdk=expected_cases,production_pipe="refus avant création des enfants",
                               nsh=doc["cases"],sha_noyau=doc["media"]["kernel"],sha_iso=doc["media"]["iso"])
            for group in range(2):
                sdk=folder/f"sdk-{group}"
                assert read(sdk/"matrice.json")["cas"]==expected_cases
                for row in read(sdk/"mesures.json"):
                    ref=host[row["case"]*4+row["mode"]]
                    assert (row["input"],row["output"])==(ref["input"],ref["output"])
                    rate=row["input"]/2**20/row["seconds"]
                    heap=row["heap"]-(1<<46)
                    assert row["matrix"]==expected_cases and 0<=heap<doc["ram"]
                    detailed.append(dict(profil=key,taille=ref["size"],traitement=ref["mode"],mios=rate,heap_bytes=heap,**row))
                    lines.append(f"{key:15} | {ref['size']:14} | {ref['mode']:15} | {row['seconds']:8.4f} | {rate:10.1f} | {row['output']:15}")
    with gzip.open(proof/"differentiel.jsonl.gz", "rt") as host_cases, gzip.open(
        proof/"matrice-par-cible.jsonl.gz", "wt", encoding="ascii"
    ) as matrix:
        for line in host_cases:
            row = json.loads(line)
            assert row["ok"]
            targets = {"darwin-arm64": {"resultat": "réussi", "statut": row["observed"]["status"]}}
            for profile in coverage:
                targets[profile] = {
                    "moteur_sdk": "réussi",
                    "attendu": "action documentaire" if row["documentation"] else "octets et statut GNU",
                    "elf_production": "admission en attente",
                }
            matrix.write(json.dumps(dict(id=row["id"], locale=row["locale"], args=row["args"],
                                         cibles=targets), ensure_ascii=True) + "\n")
    for name,value in [("mesures-qemu",detailed),("couverture-qemu",coverage)]:
        (proof/f"{name}.json").write_text(json.dumps(value,ensure_ascii=False,indent=2)+"\n")
    (proof/"tableau-qemu.txt").write_text("\n".join(lines)+"\n")
    print(f"{len(detailed)} mesures, {expected_cases} cas du moteur par profil")

if __name__=="__main__":main()
