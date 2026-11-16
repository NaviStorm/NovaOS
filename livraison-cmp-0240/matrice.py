"""Relier chaque argument testé à ses mesures et aux traces des quatre profils."""
import json
import shlex
import tarfile
from pathlib import Path
from html import escape

here = Path(__file__).resolve().parent
proof = here/"preuves"
def read(archive, name):
    with tarfile.open(proof/(archive+".tar.xz")) as tar:
        return json.load(tar.extractfile(name))
profiles = [f"{arch}-{sector}" for arch in ("aarch64","x86_64") for sector in (512,4096)]
ordinary = {}
direct = {}
for profile in profiles:
    ordinary[profile] = {r["id_corpus"]:r["statut_nsh"] for r in
                         read("rejeu-"+profile,"temoin.json")["observations"] if "id_corpus" in r}
    direct[profile] = {r["id"]:r["mesure"] for r in read("statut-"+profile,"resultats.json")["observations"]}
vectors = set(json.loads((proof/"vecteurs.json").read_text()))
for arch in ("aarch64","x86_64"):
    result = read("qual-sdk-"+arch,"resultat.json")["resultat"]
    assert result["cas"] == len(vectors) and result["sortie"] == 0
rows = json.loads((proof/"options.json").read_text())["cas"]
def literal(text):
    return "<code>"+escape(text,quote=False).replace("|","&#124;").replace("\n","&#92;n")+"</code>"
with (here/"MATRICE.md").open("w") as out:
    def line(s=""):
        print(s,file=out)
    line("# Matrice de conformité de cmp 0.24.0")
    line()
    line("Pour les deux fichiers de 262 144 octets, le premier octet de grand2 vaut 0x58 et tous les autres octets valent zéro. "
         "Avec `cmp -l grand1 grand2`, GNU 3.12 retourne **0**. Nova retourne **1** sur le Mac et sur chaque ELF de recette. "
         "La sortie complète vaut `20202020203120202030203133300a` en hexadécimal et stderr est vide. "
         "Le test attend le statut propre à chaque cible et échoue si un seul octet change.")
    line()
    line("Chaque cellule QEMU indique le statut **nsh ordinaire / processus cmp en recette**. "
         "Le tiret signale un cas non exécuté sur ce chemin. Pour les comparaisons de fichiers ordinaires, "
         "le témoin minimal du catalogue donne 127. Le SDK exerce le moteur avec entrées mémoire sur les deux ISA. "
         "Les deux sorties brutes, les entrées et la locale figurent dans [options.json](preuves/options.json). "
         "Les archives `statut-*` associent la mesure brute au PID, à la commande et au média.")
    line()
    line("| Cas | Arguments | GNU | Mach-O Nova | SDK 2 ISA | ARM 512 | ARM 4096 | x86 512 | x86 4096 |")
    line("|---|---|---:|---:|---|---|---|---|---|")
    for row in rows:
        key=row["id"]
        ref,nova=(row["mesures"][x] for x in ("GNU","Nova"))
        cells=[]
        for profile in profiles:
            actual=direct[profile].get(key)
            if actual is not None:assert actual==nova
            cells.append(f"{ordinary[profile].get(key,'—')} / {actual[0] if actual is not None else '—'}")
        values=[literal(key),literal(shlex.join(row["args"])),str(ref[0]),str(nova[0]),
                "OK" if key in vectors else "—",*cells]
        line("| "+" | ".join(values)+" |")
    line()
    line("Le chemin ordinaire documente aussi `man cmp` et la stabilité du disque. "
         "Le pipe `cat a.bin | cmp -s b.bin -` y est refusé avant création des enfants. "
         "Les archives de recette établissent également le statut 1 de `cmp a.bin b.bin` sur deux fichiers explicitement délégués.")
with (proof/"tableau-invites.txt").open("w") as out:
    print(f"{'QEMU P9':>15} | {'Taille (o)':>10} | {'Traitement':<15} | {'Nova (s)':>10} | {'Mio/s Nova':>10}",file=out)
    for profile in profiles:
        for r in read("perf-"+profile,"resultats.json")["observations"]:
            if "taille" not in r:continue
            t=r["secondes_nsh"]
            rate=f"{r['parcours_pour_debit']/(1<<20)/t:.3f}" if r["parcours_pour_debit"] else "—"
            print(f"{profile:>15} | {r['taille']:>10} | {r['traitement']:<15} | {t:10.6f} | {rate:>10}",file=out)
