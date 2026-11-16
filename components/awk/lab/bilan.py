"""Résumés dérivés des observations brutes. Arrêt en cas de binaire ou de répétition incohérents."""
import hashlib
import json
from collections import defaultdict
from pathlib import Path
from statistics import median

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT/"lab/results"


def read(name):
    return json.loads((OUT/name).read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def performance():
    columns = "corpus_Mio\tcharge\tmoteur\tMio/s_med\tMio/s_min\tMio/s_max\tenreg/s_med\tRSS_octets_max\tCPU_user_s_med\tCPU_sys_s_med\tréel_s_med"
    lines = [columns]
    candidate = digest(ROOT/"artifacts/darwin-arm64/awk")
    comparisons = read("differentiel.json")["binaries"]
    oracles = {sha for sha in comparisons.values() if sha != candidate}
    assert candidate in comparisons.values() and len(oracles) == 1
    for path in sorted(OUT.glob("final-*.jsonl")):
        meta, *jobs = [json.loads(s) for s in path.read_text().splitlines()]
        groups = defaultdict(list)
        for row in jobs:
            expected = candidate if Path(row["executable"]).name == "awk" else next(iter(oracles))
            assert row["ok"] and row["sha256"] == expected
            groups[(row["workload"],Path(row["executable"]).name)].append(row)
        for (charge,exe), group in groups.items():
            assert [r["repeat"] for r in group] == list(range(4))
            trials = group[1:]
            rates = [r["mib_s"] for r in trials]
            metrics = [median(rates), min(rates), max(rates),
                       median(r["records_s"] for r in trials),max(r["rss_bytes"] for r in trials),
                       *[median(r[k] for r in trials) for k in ("user","system","seconds")]]
            lines.append("\t".join([str(meta["bytes"]/2**20),charge,exe,*[f"{m:.5f}" for m in metrics]]))
    (OUT/"performances.tsv").write_text("\n".join(lines)+"\n")


def options():
    data = read("options.json")
    groups = defaultdict(list)
    for r in data["cas"]:
        groups[r["option"]].append(r)
    header = ["# Matrice des options de GNU awk 5.4.1", "",
              "Oracle GNU ARM64 du banc. Les formes courtes et longues, séparées ou attachées, figurent avec leurs arguments, sorties, diagnostics, fichiers et statuts dans [la mesure brute](lab/results/options.json). Une égalité porte sur le cas exercé. Les restrictions du langage et des services sont consignées dans [le rapport](QUALIFICATION.md).", "",
              "| Option courte | Option longue | Formes exercées | Résultat du candidat Darwin |",
              "|---|---|---:|---|"]
    notes = {"b":"Traitement par octets, y compris NUL et octets invalides dans le différentiel.",
             "C":"Notice MIT du moteur.", "h":"Aide ARC-026 en français ou en anglais.",
             "V":"Version propre du moteur en français ou en anglais.",
             "S":"Exécution admise. Les quatre opérations interdites produisent une erreur dans le différentiel.",
             "l":"Chargement dynamique refusé avant exécution, y compris pour un module nommé.",
             "i":"Recherche AWKPATH et inclusion unique. Les chemins par défaut GNU installés ne sont pas embarqués.",
             "Y":"Option conditionnelle du code source GNU. Refus dans les deux exécutables testés.",
             "Z":"Option conditionnelle du code source GNU. Refus dans les deux exécutables testés."}
    for desc, rows in groups.items():
        if desc=="syntaxe":continue
        short,long,*_ = desc.split()
        default = "Concordance des formes exercées." if all(not r["differences"] for r in rows) else "Refus explicite du candidat (statut 2)."
        header.append(f"| `-{short}` | `--{long}` | {len(rows)} | {notes.get(short,default)} |")
    header += ["","Le séparateur `--` est accepté. Les sondes du commutateur interne `-G`, de `-W version`, des options groupées `-bh` et de l’abréviation `--vers` sont également archivées. Le candidat exige les noms longs entiers et refuse les groupes courts. Les formes de `--persist` ont un diagnostic propre à l’oracle.", ""]
    (ROOT/"OPTIONS.md").write_text("\n".join(header).replace(chr(92)+chr(96),chr(96)))


if __name__ == "__main__":
    performance()
    options()
