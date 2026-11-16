"""Matrice des options comparées. Vérifie les graphies du manuel Info de GNU grep 3.12."""
import json
from pathlib import Path
import re
from campagne import OPTIONS

base = Path(__file__).resolve().parent
reports = [json.loads((base/f"{lang}.json").read_text()) for lang in ("C","en_US.UTF-8")]
manual = Path("/opt/homebrew/opt/grep/share/info/grep.info").read_text(errors="replace")
def key(form):
    if form == "-1": return "-NUM"
    return re.split(r"[=\[ ]",form)[0]
forms = {key(o) for group,_,_ in OPTIONS for o in group.split()}
documented = {key(m) for m in re.findall(r"^‘(-[^’]+)’\s*$",manual,re.MULTILINE)}
assert documented <= forms, documented - forms
rows = ["# Matrice GNU grep 3.12 sur Darwin ARM64","",
    "Chaque graphie a été invoquée avec son propre témoin. « Identique » exige le même statut, les mêmes octets sur stdout et les mêmes octets sur stderr. Les noms abrégés dans les globes et les valeurs d’options illustrent les arguments effectivement testés. Chaque JSON fournit argv, l’empreinte de stdin et les deux résultats hexadécimaux. Un résultat porte sur le témoin et sa locale.","",
    "| Options et alias exercés | Autres arguments du témoin | C | en_US.UTF-8 |",
    "|---|---|---|---|"]
for group,args,_ in OPTIONS:
    cells = []
    for report in reports:
        index = {r["test"]:r for r in report["mesures"]}
        exact = sum(index[o]["identique"] for o in group.split())
        cells.append("Identique" if exact == len(group.split()) else f"{exact}/{len(group.split())} identique (texte ARC-026)" if group in ("--help","-V --version") else f"Écart ({exact}/{len(group.split())})")
    esc = lambda s:s.replace("|",r"\|")
    rows.append("| "+" | ".join([" ".join("\x60"+o+"\x60" for o in group.split()),"\x60"+esc(" ".join(args))+"\x60" if args else "—",*cells])+" |")
rows.extend(["","## Témoins complémentaires","",
    "Les combinaisons, refus de permission, erreurs de regex et de syntaxe, EOF, FIFO, SIGPIPE, vidage avant EOF, position de stdin régulier après -m, contextes, UTF-8 malformé et pics mémoire ont des témoins distincts. Le corpus généré varie les quatre dialectes, les options et les octets d’entrée avec une graine enregistrée.",""])
for report in reports:
    r=report["mesures"]
    rows.append(f"- {report['locale']} · {len(r)} comparaisons, {sum(x['identique'] for x in r)} identiques. Écarts observés : "+", ".join("\x60"+x["test"]+"\x60" for x in r if not x["identique"])+".")
(base/"matrice.md").write_text("\n".join(rows)+"\n")
