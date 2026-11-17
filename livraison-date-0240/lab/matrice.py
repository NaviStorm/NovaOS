"""Produit une vue vérifiable de chaque comparaison, y compris les cellules non exercées."""
import base64
import csv
import gzip
import json
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
PROOF = HERE / "preuves"
output = HERE / "matrice.tsv"
profiles = [f"{isa}-{size}" for isa in ("aarch64","x86_64") for size in (512,4096)]

def text(data):
    return json.dumps(base64.b64decode(data).decode("utf-8","backslashreplace"),ensure_ascii=False)

def verdict(result):
    return f"{'OK' if result['ok'] else 'ÉCART'} rc={result['statut']} {result['oracle']}"

cells = {}
for profile in profiles:
    with gzip.open(PROOF / f"sdk-{profile}.json.gz", "rt") as stream:
        for case in json.load(stream)["cas"]:
            cells.setdefault(case["id"], {})[profile] = case
with output.open("w") as out:
    writer = csv.writer(out, delimiter="\t", lineterminator="\n")
    writer.writerow(["identifiant","catégorie","arguments (JSON)","TZ/locale","statut GNU","stdout GNU","stderr GNU","effet GNU (base64)","statut Nova Darwin","stdout Nova","stderr Nova","effet Nova (base64)","résultat Darwin",*profiles])
    for line in (PROOF/"options.jsonl").read_text().splitlines():
        r=json.loads(line)
        fields=[r["id"],r["kind"],json.dumps(r.get("argv",[]),ensure_ascii=False),r.get("TZ",r.get("locale",r.get("tag","")))]
        for key in ("gnu","nova"):
            v=r.get(key)
            fields.extend([v[0],text(v[1]),text(v[2]),v[3] if len(v)>3 else ""] if v else ["","","",""])
        observed=cells.pop(r["id"],{})
        writer.writerow([*fields,"OK" if r["ok"] else "ÉCART",*[verdict(observed[p]) if p in observed else "non exercé dans le catalogue" for p in profiles]])
    for key, observed in cells.items():
        first=next(iter(observed.values()))
        writer.writerow([key,"catalogue",json.dumps(first["argv"]),*[""]*10,*[verdict(observed[p]) for p in profiles]])
