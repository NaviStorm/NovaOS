from options import DATA
from laboratoire import *
import struct
import re
def build(out):
    pool={};body=bytearray()
    def n(x):body.extend(struct.pack("<I",x))
    def v(data):
        if isinstance(data,str):data=data.encode()
        n(pool.setdefault(data,len(pool)))
    nodes={k:(v,3 if k=="protected" else 0) for k,v in DATA.items()}
    for k in DATA:
        for parent in Path(k).parents:
            if parent!=Path("."):nodes[str(parent)]=(b"",1)
    for k,d in [("syma","a"),("symb","b"),("dangling","missing")]:nodes[k]=(d,2)
    n(len(nodes))
    for k,(value,kind) in nodes.items():v(k);v(value);n(kind)
    rows=json.loads((HERE/"preuves/complet/options.json").read_text())["cas"]
    # Le paginateur Darwin externe insère l'heure courante. Le profil invité a son propre oracle.

    n(len(rows))
    for r in rows:
        doc=r["reference"]=="ARC-026";res=r["Nova"] if doc and r["ok"] else r["GNU"]
        n(r["id"]);n(len(r["args"]))
        for arg in r["args"]:v(arg)
        v(bytes.fromhex(r["stdin"]));v(r["env"].get("LANG","en"));n(res[0]);stdout=bytes.fromhex(res[1])
        if any(a in ("-l","--paginate") for a in r["args"]):
            stdout=re.sub(rb"(?m)^\w{3} [ 0-9]\d \d\d:\d\d \d{4} (?=diff )",b"Nov 14 22:13 2023 ",stdout)
        v(stdout);v(bytes.fromhex(res[2]));n(doc)
    out.write_bytes(struct.pack("<I",len(pool))+b"".join(struct.pack("<I",len(b))+b for b in pool)+body)
    return [r["id"] for r in rows]
if __name__=="__main__":build(HERE/"preuves/vecteurs.bin")
