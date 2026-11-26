"""Qualification Darwin de la spécification Ethernet. Un processus neuf par série/cellule."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

HERE=Path(__file__).resolve().parent

def output(args):
    return subprocess.check_output([str(a) for a in args],text=True).strip()

def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f,"sha256").hexdigest()

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("archive",type=Path)
    args=parser.parse_args()
    archive=args.archive.resolve()
    archive.mkdir(parents=True,exist_ok=False)
    subprocess.run([sys.executable,"-B",HERE/"vecteurs.py"],check=True)
    meta={"systeme":platform.platform(),"rustc":output(["rustup","run","stable","rustc","-vV"]),
          "clang":output(["xcrun","clang","--version"]),
          "sources":{f.name:digest(f) for f in HERE.iterdir() if f.suffix in (".rs",".c",".bin",".py")},
          "commit":output(["git","-C",HERE,"rev-parse","HEAD"])}
    (archive/"environnement.json").write_text(json.dumps(meta,indent=2,ensure_ascii=False)+"\n")
    for isa,arch in [("aarch64","arm64"),("x86_64","x86_64")]:
        run=archive/isa
        run.mkdir()
        target=isa+"-apple-darwin"
        rust=["rustup","run","stable","rustc","--edition","2024","-O","--target",target]
        launches=["/usr/bin/arch","-"+arch]
        commands=[]
        def build(cmd):
            commands.append([str(a) for a in cmd])
            with (run/"construction.log").open("a") as f:
                subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,check=True)
        build(rust+["--crate-name","ethernet_v1","--crate-type","rlib",HERE/"lib.rs","-o",run/"libethernet_v1.rlib"])
        build(rust+["--test",HERE/"lib.rs","-o",run/"conformite"])
        build(["xcrun","clang","-arch",arch,"-O2","-Wall","-Wextra","-Werror","-c",HERE/"usage_darwin.c","-o",run/"usage.o"])
        build(rust+[HERE/"bench.rs","--extern","ethernet_v1="+str(run/"libethernet_v1.rlib"),"-C","link-arg="+str(run/"usage.o"),"-o",run/"transport"])
        (run/"commandes.json").write_text(json.dumps(commands,indent=2)+"\n")
        evidence={"uname":output(launches+["/usr/bin/uname","-m"]),
                  "binaire":output(["file",run/"transport"]),
                  "translation":output(launches+["/usr/sbin/sysctl","-in","sysctl.proc_translated"])}
        if isa=="x86_64": assert evidence["translation"]=="1",evidence
        (run/"execution.json").write_text(json.dumps(evidence,indent=2)+"\n")
        tests=output(launches+[run/"conformite","--test-threads=1"])
        (run/"tests.txt").write_text(tests+"\n")
        rows=[]
        for series in range(3):
            modes=["data","boite","fenetre"]
            modes=modes[series:]+modes[:series]
            for b in [1,2,4,8]:
                for length in [60,1514]:
                    for mode in modes:
                        lines=output(launches+[run/"transport",mode,str(b),str(length),str(series+1)]).splitlines()
                        assert len(lines)==3 and lines[0].startswith("CELL ")
                        row={}
                        for part in lines[0].split()[1:]:
                            key,value=part.split("=")
                            row[key]=value if key=="mode" else int(value)
                        row["ns"]=json.loads(lines[1].removeprefix("NS "))
                        row["retrait_ns"]=json.loads(lines[2].removeprefix("REVOKE "))
                        assert (row["mode"],row["b"],row["len"])==(mode,b,length)
                        assert len(row["ns"])==4096 and len(row["retrait_ns"])==129
                        rows.append(row)
        (run/"series.json.gz").write_bytes(gzip.compress(json.dumps(rows,separators=(",",":")).encode(),mtime=0))
        manifest={p.name:digest(p) for p in sorted(run.iterdir())}
        (run/"sha256.json").write_text(json.dumps(manifest,indent=2)+"\n")
        print(isa,tests.splitlines()[-1],len(rows),"séries",flush=True)

if __name__=="__main__":main()
