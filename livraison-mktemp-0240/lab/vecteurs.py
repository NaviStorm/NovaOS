"""Projeter l’oracle Darwin sur les champs observables par le demandeur P9."""
import json
from pathlib import Path
import re

def generate(matrix, destination):
    rows=json.loads(matrix.read_text())
    output=[]
    ids=[]
    skipped={}
    for i,row in enumerate(rows):
        assert row["ok"]
        try: words=[bytes.fromhex(w).decode() for w in row["argv"]]
        except UnicodeDecodeError:
            skipped[i]="argv non UTF-8";continue
        if row["closed"] or "POSIXLY_CORRECT" in row["env"] or any("refus/" in w for w in words):
            skipped[i]="descripteur POSIX, environnement POSIX ou chmod Darwin";continue
        env=row["env"]
        pat=row["pattern"]
        rc=row["gnu"]["rc"]
        effect=row["gnu"]["effect"]
        if pat:
            expected=re.sub(r"\[a-zA-Z0-9\]\{(\d+)\}",r"@\1@",pat).replace(r"\.",".")
            mode="nom"
        elif row["doc"]:
            expected=bytes.fromhex(row["nova"]["out"]).decode()
            mode="egal"
        else:
            expected=bytes.fromhex(row["gnu"]["err"]).decode().removeprefix("g")
            mode="egal"
            if "failed to create" in expected:
                expected="mktemp: "
                mode="prefixe" # Causes POSIX et codes de stockage natifs distincts.
        fields=[env.get("LC_ALL","C"),env.get("TMPDIR","env"),expected,mode]
        fmt=lambda v:json.dumps(v,ensure_ascii=False)
        output.append("(&["+",".join(map(fmt,words))+"],"+",".join(map(fmt,fields))+f",{rc},{2 if effect and effect[0]=='dir' else 1 if effect else 0}),")
        ids.append(i)
    destination.write_text("&[\n"+"\n".join(output)+"\n]\n")
    return dict(indices=ids,exclus=skipped)
