"""Dictionnaire de blocs et références u32 little-endian, utilisé par le témoin SDK."""
import gzip
import json
import os
from pathlib import Path
import struct
import sys

def generate(source,target):
    pool={}
    rows=bytearray()
    count=0
    def index(data):
        return pool.setdefault(data,len(pool))
    for line in gzip.open(source,"rt"):
        row=json.loads(line)
        assert row["ok"]
        ref=row["gnu"]
        raw=bytes.fromhex(ref["stdout"])
        doc=row["documentation"]
        kind=(2 if raw.startswith(b"tr (GNU coreutils)") else 1) if doc else (3 if ref["status"] else 0)
        err=bytes.fromhex(ref["stderr"])
        err=b"\n".join(b"tr: "+s[5:] if s.startswith(b"gtr: ") else s for s in err.split(b"\n"))
        args=row["args"]
        rows.extend(struct.pack("<BBIBIII",int(row["locale"]!="C"),kind,row["chunk"],len(args),
                                index(bytes.fromhex(row["stdin"])),index(b"" if doc else raw),index(err)))
        for arg in args: rows.extend(struct.pack("<I",index(os.fsencode(arg))))
        count+=1
    with target.open("wb") as out:
        out.write(struct.pack("<II",len(pool),count))
        for data in pool: out.write(struct.pack("<I",len(data))+data)
        out.write(rows)
    return count

if __name__=="__main__": generate(Path(sys.argv[1]),Path(sys.argv[2]))
