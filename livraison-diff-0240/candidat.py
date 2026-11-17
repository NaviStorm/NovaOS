"""Comparer le candidat uutils aux observations GNU de la matrice, hors documentation Nova."""
from options import setup
from laboratoire import *
import sys
p=Path(sys.argv[1]).resolve()
rows=json.loads((HERE/"preuves/complet/options.json").read_text())["cas"]
data=[]
with tempfile.TemporaryDirectory(dir=TMP,prefix="diff-uutils-") as td:
    work=Path(td);setup(work)
    for r in rows:
        if r["reference"]!="GNU":continue
        result=execute(p,r["args"],work,bytes.fromhex(r["stdin"]),r["env"])
        data.append(dict(id=r["id"],ok=result==r["GNU"],statut=result[0],
                         stdout_sha256=hashlib.sha256(bytes.fromhex(result[1])).hexdigest(),
                         stderr_sha256=hashlib.sha256(bytes.fromhex(result[2])).hexdigest()))
save(HERE/"preuves/uutils.json",dict(binaire_sha256=digest(p),essais=data))
print(sum(r["ok"] for r in data),"/",len(data))
