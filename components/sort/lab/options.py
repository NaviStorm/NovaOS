"""Différentiel par graphie. Chaque moteur reçoit le même argv[0] et un répertoire neuf."""
from itertools import groupby
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from collections import Counter

ROOT=Path(__file__).resolve().parents[1]
REFERENCE=Path("/opt/homebrew/bin/gsort")
GNU=REFERENCE.resolve().parents[1]/"libexec/gnubin/sort"
assert GNU.samefile(REFERENCE)
DEFAULT=b" 10\n2\n01\n-2\n10\n1.01\n1.0100\n-100000000000000000000001\n1000000000000000000000000\n"
NAMES=[
 ("b","ignore-leading-blanks",b" z\na\n"),
 ("d","dictionary-order",b".Z\nY\nA;\nA!\n"),
 ("f","ignore-case",b"b\nA\na\nZ\n"),
 ("g","general-numeric-sort",b"1e2\n-Inf\nxxx\n-3\nNaN\n0x1.4p2\n"),
 ("i","ignore-nonprinting",b"\x7fB\na\n\x04Z\n"),
 ("M","month-sort",b"Dec\njan\n???\nNOV\n"),
 ("h","human-numeric-sort",b"2M\n3K\n10000K\n0\n-2G\n"),
 ("n","numeric-sort",DEFAULT),("R","random-sort",b"a\nb\nc\na\nb\n"),
 ("r","reverse",b"b\nc\na"),("V","version-sort",b".\n..\nv1~rc\nv10\nv2\nv2.tar.gz\n")]
def cases():
    for short,long,data in NAMES:
        for flag in ["-"+short,"--"+long]:
            yield flag,[flag],data,"random" if short=="R" else "exact"
    for word in ["general-numeric","human-numeric","month","numeric","random","version"]:
        yield "--sort="+word,["--sort="+word],DEFAULT,"random" if word=="random" else "exact"
    for name,flags,data in [
        ("empty",[],b""),("binary",[],b"\xff\n\x00a\nz"),
        ("- operand",["-","a"],DEFAULT),("several",["a","b"],b""),
        ("--",["--","-z"],b""),("stdin files0",["--files0-from=-"],b"a\0b\0"),
        ("files0",["--files0-from=list"],b""),("files0 empty",["--files0-from=empty"],b""),
        ("files0 extra",["--files0-from=list","a"],b""),
        ("files0 null name",["--files0-from=-"],b"a\0\0"),
        ("-o collision",["-o","a","a","b"],b""),("--output",["--output=out","a","b"],b""),
        ("-o hardlink merge",["-m","--output=link","a","b"],b""),
        ("bad output",["-o","missing/out","a"],b""),("missing",["absent"],b""),
        ("read error",["."],b""),("-m",["-m","a","b"],b""),("--merge",["--merge","a","b"],b""),
        ("merge unsorted",["-m"],b"z\na\n"),("-z",["-z"],b"X\n\0a\0\0"),
        ("--zero-terminated",["--zero-terminated"],b"z\0b\0\0"),
        ("check good",["-c"],b"a\nb\n"),("check bad",["-c"],b"b\na\n"),
        ("check quiet",["-C"],b"b\na"),("check unique",["-cu"],b"a\na\n"),
        ("-s",["-s","-k1,1n"],b"1 z\n1 a\n0 x\n"),
        ("--stable",["--stable","-k1,1n"],b"1 z\n1 a\n0 x\n"),
        ("-u",["-uk1,1n"],b"1 z\n1 a\n0 x\n"),("--unique",["--unique","-k1,1n"],b"1 z\n1 a\n0 x\n"),
        ("key local",["--key=1.2b,1.3r","-k2,2n","-t:"],b"  b:11\n  b:2\n  a:9\n"),
        ("empty fields",["--field-separator=:","-k2,2"],b"z:\n:x\na::q\n"),
        ("--debug",["--debug","-k1,1"],b"c a\nb z\n"),
        ("16 MiB line",["-S1M"],b"x"*(16<<20)+b"\na\n"),
        ("multipass stable",["-S4K","--batch-size=2","-s","-k1,1"],b"".join(f"{i%5} {i:05}\n".encode() for i in range(1800))),
        ("multipass unique",["-S4K","--batch-size=2","-u","-k1,1"],b"".join(f"{i%5} {i:05}\n".encode() for i in range(1800))),
        ("gzip",["-S4K","--compress-program=gzip","-T","tmp1","--temporary-directory=tmp2"],DEFAULT*200),
        ("compression failure",["-S4K","--compress-program=false"],DEFAULT*200),
        ("directory invalid",["-T","absent","-S4K"],DEFAULT*200),
        ("parallel",["--parallel=2"],DEFAULT*20),
        ("--buffer-size",["--buffer-size=2%"],DEFAULT),
        ("--random-source",["-R","--random-source=seed"],DEFAULT*3),
        ("seeded C order",["-R","--random-source=seed"], b"a\nb\nc\nd\ne\nf\n"),
        ("short seed",["-R","--random-source=a"],DEFAULT),
        ("random absent",["-R","--random-source=absent"],DEFAULT),
        ("legacy -y",["-y","10","-n"],DEFAULT),("legacy -y attached",["-yanything"],DEFAULT),("legacy -y missing",["-y"],DEFAULT),
        ("invalid check",["--check=bad"],DEFAULT),("invalid sort",["--sort=bad"],DEFAULT),
        ("--help",["--help"],b""),("--version",["--version"],b""),
        ("check missing",["-c","absent"],b""),("check with -o",["-C","-o","out"],b""),
        ("bad batch",["--batch-size=1"],b""),("bad workers",["--parallel=0"],b""),
        ("bad budget",["-S1X"],b""),("bad key",["-k0.0"],b""),
        ("bad separator",["-t::"],b""),("conflict",["-hn"],b""),
        ("unknown",["--bad-option"],b""),("missing arg",["-k"],b""),
        ("UTF8-collation",[],"é\nz\na\nÉ\nè".encode()),
    ]:
        yield name,flags,data,("random" if name=="--random-source" else "doc" if name in ("--help","--version") else "exact")
    for flag in ["--check","--check=diagnose-first","--check=quiet","--check=silent"]:
        yield flag,[flag],b"b\na","exact"
    for suffix in ["b","K","M","G","T","P","E","Z","Y","R","Q","%",""]:
        yield "budget "+suffix,["-S","1"+suffix],DEFAULT,"exact"

def invoke(exe,flags,data,work,locale):
    with tempfile.TemporaryDirectory(dir=work) as tmp:
        path=Path(tmp)
        for name,value in {"a":b"a\nc\n","b":b"b\nd\n","-z":b"z\na\n","list":b"a\0b\0","seed":bytes(range(256))*8,"empty":b""}.items():
            (path/name).write_bytes(value)
        os.link(path/"a",path/"link")
        for d in ["tmp1","tmp2"]:(path/d).mkdir()
        proc=subprocess.run(["sort",*flags],executable=str(exe),cwd=path,input=data,capture_output=True,
                            env=dict(os.environ,LC_ALL=locale,TMPDIR=str(path/"tmp1")),timeout=45)
        outputs={n:(path/n).read_bytes().hex() for n in ("a","link","out") if (path/n).exists()}
        leftovers=[f.name for d in ("tmp1","tmp2") for f in (path/d).iterdir()]
        return proc.returncode,proc.stdout,proc.stderr,outputs,leftovers

def brief(v):
    code,out,err,files,left=v
    return dict(status=code,stdout_sha256=hashlib.sha256(out).hexdigest(),stdout_bytes=len(out),
                stdout_sample=out[:256].hex(),stderr=err.decode(errors="backslashreplace"),
                files=files,leftovers=left)
def main():
    p=argparse.ArgumentParser()
    p.add_argument("--candidate",type=Path,default=ROOT/"artifacts/darwin-arm64/sort")
    p.add_argument("--engine",choices=["gnu","nova","both"],default="both")
    p.add_argument("--work",type=Path,default=Path(os.environ.get("TMPDIR",".")))
    p.add_argument("--output",type=Path)
    opts=p.parse_args()
    result=[]
    for name,flags,data,mode in cases():
        locale="en_US.UTF-8" if name=="UTF8-collation" else "C"
        row=dict(case=name,args=flags,locale=locale,stdin_bytes=len(data),stdin_sha256=hashlib.sha256(data).hexdigest(),mode=mode)
        values=[]
        for engine,exe in [("gnu",GNU),("nova",opts.candidate.resolve())]:
            if opts.engine not in ("both",engine):continue
            val=invoke(exe,flags,data,opts.work,locale)
            row[engine]=brief(val)
            values.append(val)
        if len(values)==2:
            a,b=values
            if mode=="random":
                records=b[1].splitlines()
                ok=Counter(data.splitlines())==Counter(records) and len(list(groupby(records)))==len(set(records))
                output=ok
            elif mode=="doc":
                output=b"0.1.0" in b[1] if name=="--version" else all(opt.encode() in b[1] for _,opt,_ in NAMES)
            else:output=a[1]==b[1]
            row["equal"]=dict(status=a[0]==b[0],stdout=output,stderr=a[2]==b[2],files=a[3]==b[3],clean=not a[4] and not b[4])
        result.append(row)
    text=json.dumps(result,ensure_ascii=False,indent=2)
    if opts.output:opts.output.write_text(text+"\n")
    for row in result:
        dif=[k for k,v in row.get("equal",{}).items() if not v]
        print(f"{row['case']:24} {', '.join(dif) or ('OK' if 'equal' in row else row[opts.engine]['status'])}")
    return int(any(not all(row["equal"].values()) for row in result if "equal" in row))
if __name__=="__main__":
    raise SystemExit(main())
