"""Lire les rapports effectifs des quatre cellules et vérifier leur rattachement aux ELF livrés."""
import base64
import gzip
import hashlib
import json
from pathlib import Path
from statistics import median
import sys

from options import HERE

def load(path):
    with gzip.open(path,"rt") as stream:
        return json.load(stream)

def summary(directory):
    root=directory/"qemu"
    mac=json.loads((directory/"options.json").read_text())
    assert all(row["ok"] for row in mac)
    print("# Résultats de qualification de uname\n")
    print(f"Darwin ARM64 a réussi {len(mac)}/{len(mac)} cas. Les statuts et les deux flux sont conservés pour GNU et Nova. [Mesures macOS en regard](preuves/darwin.md)")
    print("\n| Invité P9 | Cas ELF conformes | Commandes nsh vérifiées | Écritures / octets / flush pendant les consultations |")
    print("|---|---:|---:|---|")
    reports={}
    for arch in ("aarch64","x86_64"):
        elf=(HERE/f"bin/uname-{arch}.elf").read_bytes()
        for sector in (512,4096):
            key=f"{arch}-{sector}"
            sdk=load(root/f"sdk-{key}.json.gz")
            shell=load(root/f"nsh-{key}.json.gz")
            assert sdk["elf"]==hashlib.sha256(elf).hexdigest()
            assert all(row["ok"] for row in sdk["cas"])
            assert sdk["banniere"]=="NovaOS v0.20.0" and shell["avant"]==shell["apres"]
            assert len(sdk["performance"])==6
            for arch_option,want in [("-m",arch),("-r","0.20.0"),("-v",sdk["banniere"]),("-s","NovaOS")]:
                case=next(r for r in sdk["cas"] if r["argv"]==[arch_option.encode().hex()] and r["ports"]==6)
                assert base64.b64decode(case["stdout"])==(want+"\n").encode()
            reports[key]=sdk
            print(f"| {arch} / {sector} | {len(sdk['cas'])} | {len(shell['cas'])} | 0 / 0 / 0 |")
    print("\n## ELF NovaOS sous QEMU\n")
    print("Médianes en millisecondes. Chaque case donne le trajet complet puis, entre parenthèses, le temps d’admission et de chargement inclus dans le total. Trois passages suivent un échauffement. La fréquence et les compteurs bruts figurent dans chaque rapport SDK.\n")
    print("| Sélection | Appels | AArch64 / 512 | AArch64 / 4096 | x86_64 / 512 | x86_64 / 4096 |")
    print("|---|---:|---:|---:|---:|---:|")
    for arg in ["-s","-a","-omrs"]:
        for count in [1,8]:
            line=[arg,str(count)]
            for sdk in reports.values():
                row=next(r for r in sdk["performance"] if r["arg"]==arg and r["appels"]==count)
                clocks=row["compteurs"][1:]
                admission=median(r["admission"]/r["hz"] for r in clocks)
                line.append(f'{1000*row["mediane"]:.3f} ({1000*admission:.3f})')
            print("| "+" | ".join(line)+" |")
    print("\n## Couverture par orthographe\n")
    print("Nombre d’exécutions réussies du cas isolé dans les journaux. Une erreur attendue constitue un test conforme. Chaque cellule QEMU recompte les argv observés par le contrôleur SDK.\n")
    print("| Orthographe | Darwin | AArch64 / 512 | AArch64 / 4096 | x86_64 / 512 | x86_64 / 4096 |")
    print("|---|---:|---:|---:|---:|---:|")
    from options import NAMES
    for option in ["","--",*("-"+c for c in "aAsnrvmpio"),*("--"+n for n in NAMES)]:
        args=[option.encode().hex()] if option else []
        counts=[sum(r["argv"]==args and r["ok"] and r["mode"]=="normal" for r in mac)]
        counts.extend(sum(r["argv"]==args and r["ok"] and r["ports"]==6 for r in v["cas"]) for v in reports.values())
        assert all(counts),(option,counts)
        print("| "+(f"`{option}`" if option else "défaut")+" | "+" | ".join(map(str,counts))+" |")
    print("\nLes matrices complètes incluent les combinaisons, diagnostics et flux. [Darwin](preuves/options.json) · [Résultats, séries UART et médias QEMU](preuves/qemu)")

if __name__=="__main__": summary(Path(sys.argv[1]) if len(sys.argv)>1 else HERE/"preuves")
