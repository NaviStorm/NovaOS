"""Rassembler les observations d'un rejeu complet et les indexer par option."""
from laboratoire import *
import statistics
import sys
base=Path(sys.argv[1])
out=HERE/"preuves"
tests=json.loads((out/"complet/options.json").read_text())["cas"]
mat={}
for row in tests:mat.setdefault(row["option"],[]).append(row)
cols=[];data={};metrics=[]
for arch in ["aarch64","x86_64"]:
    build=json.loads((base/f"lab-{arch}/provenance.json").read_text())
    for sector in [512,4096]:
        profile=f"{arch}-{sector}";cols.append(profile)
        for prefix in ["","ord-"]:
            case=base/(prefix+profile);dest=out/"qemu"/(prefix+profile);dest.mkdir(parents=True,exist_ok=True)
            for fname in ["resultats.json","images.json","session.vm.json","session.serial","disque-documentation.json"]:
                src=case/fname
                if src.is_file():shutil.copy2(src,dest/fname)
            save(dest/"fichiers-sha256.json",{f.name:digest(f) for f in sorted(case.iterdir()) if f.is_file()})
        case=base/profile
        assert digest(case/"diff-recette.elf")==build["probe"]
        vm=json.loads((case/"session.vm.json").read_text());assert vm["elf_sha256"]==build["noyau"]
        save(out/"qemu"/profile/"construction.json",build)
        result=json.loads((case/"resultats.json").read_text());assert result[0]["cas"]==len(tests)
        data[profile]=set(result[0]["divergences"])
        assert len(data[profile])==result[0]["ecarts"] and result[0]["statut"]==int(bool(data[profile]))
        for r in result[1:]:
            metrics.append(f"| {profile} | {r['charge']} | {r['entree']} | {r['octets_diff']} | {r['mediane_ms']:.2f} | {r['mediane_mio_s']:.4f} | {max(r['pic_tas_octets'])//1024} |")
table=["| Option, alias ou groupe | Darwin | "+" | ".join(cols)+" |","|---|---:|---:|---:|---:|---:|"]
for name,rows in mat.items():
    cells=[sum(r["ok"] for r in rows),*(sum(r["id"] not in data[k] for r in rows) for k in cols)]
    table.append("| "+name+" | "+" | ".join(f"{cnt}/{len(rows)}" for cnt in cells)+" |")
(out/"MATRICE.md").write_text("# Cas conformes par cible\n\nChaque cellule donne le nombre de cas conformes au statut et aux deux flux de référence. Les colonnes invitées concernent le moteur dans l’ELF de recette. Les identifiants, valeurs, combinaisons, opérandes, entrées standard et octets attendus sont dans [le relevé Darwin](complet/options.json). Les identifiants en écart sont journalisés individuellement par chaque invité. Le [rapport](../README.md) décrit les essais sur les fichiers P9 réels et le shell ordinaire.\n\n"+"\n".join(table)+"\n")
(out/"QEMU.md").write_text("# Performances des ELF de recette\n\nMédiane de trois lancements après un passage de contrôle. Le temps entre l’envoi de la commande et le retour du prompt inclut le lancement, les E/S P9 et la transmission du résultat sur la console de recette. Le débit compte tous les octets des fichiers des deux opérandes. Le pic mesure les pages de tas du processus attribuées par HEAP_RESIZE jusqu’à la fin de la comparaison. La pile, le code, le noyau et QEMU sont exclus.\n\n| ISA et secteur P9 | Charge | Entrée o | Diff o | ms | Mio/s | Pic tas Kio |\n|---|---|---:|---:|---:|---:|---:|\n"+"\n".join(metrics)+"\n")
