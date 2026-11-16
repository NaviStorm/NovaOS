#!/usr/bin/env python3
"""Compare chaque canal et le statut en locale C, sans décoder les noms."""
import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile

GNU = "/opt/homebrew/bin/gsha256sum"
p = argparse.ArgumentParser(description=__doc__)
p.add_argument("candidate")
args = p.parse_args()
candidate = str(Path(args.candidate).resolve())
os.environ["LC_ALL"] = "C"
count = 0

def run(tool, argv, data=b""):
    # argv[0] identique pour comparer jusqu'aux messages de getopt.
    r = subprocess.run(["sha256sum", *argv], executable=tool, input=data, capture_output=True)
    return (r.returncode, r.stdout, r.stderr.replace(b"gsha256sum:", b"sha256sum:").replace(b"gsha256sum --help", b"sha256sum --help"))

def compare(argv, data=b""):
    global count
    ref, actual = [run(tool,argv,data) for tool in (GNU,candidate)]
    assert ref == actual,(argv,data[:100],ref,actual)
    count += 1
    return actual

with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as td:
    os.chdir(td)
    Path("dir").mkdir()
    Path("elsewhere").mkdir()
    names=["data","a b","a\\b","a\nb","a\rb","-dash","dir/one","dir/two"]
    raw=b"invalid-\xff"
    try:
        open(raw,"wb").close()
        names.append(raw)
    except OSError as e:
        print(f"nom hors UTF-8 refusé par le volume hôte ({e.errno})")
    corpus=bytes(range(256))*517+b"\x00\r\n\xff"
    for n in names:
        with open(n,"wb") as f:f.write(corpus)
    for size in [0,3,55,56,63,64,65,65535,65536,65537,len(corpus)]:
        data=corpus[:size]
        assert compare([],data)[1]==hashlib.sha256(data).hexdigest().encode()+b"  -\n"
    compare(["-","data","-"],b"abc")
    for flag in ([],["-b"],["--binary"],["-t"],["--text"],["--tag"],["--tag","-b"],["-z"],["--zero"],["-bz"],["--tag","-z"]):
        line=compare([*flag,"--",*names])[1]
        if b"\0" in line:continue
        for writer,reader in ((candidate,GNU),(GNU,candidate)):
            manifest=run(writer,[*flag,"--",*names])[1]
            Path("elsewhere/SUMS").write_bytes(manifest)
            assert run(reader,["-c","elsewhere/SUMS"])[0]==0
        compare(["--check","elsewhere/SUMS"])
        compare(["-c","-"],line)
    good=run(GNU,["data"])[1]
    missing=good.replace(b"  data",b"  absent")
    mismatch=b"0"*64+b"  data\n"
    unread=good.replace(b"  data",b"  dir")
    Path("protected").write_bytes(corpus)
    os.chmod("protected",0)
    unread_file=good.replace(b"  data",b"  protected")
    assert run(GNU,["-c"],unread_file)[0]==1
    for sample in [b"",b"oops",good+b"oops\n",b"wrong\n"+good,good[:-1],good*3+mismatch,missing,missing+good,unread,unread_file,good[:63]+good[64:],b"# note\n\n"+good,good.upper()[:64]+good[64:]]:
        for flag in ([],["--strict"],["-w"],["--warn"],["--status"],["--quiet"],["--ignore-missing"],["--ignore-missing","--status"],["--warn","--quiet"],["--quiet","--warn"],["--status","--warn","--strict"]):
            compare(["-c",*flag],sample)
    for flag in ("-b","--binary","-t","--text","--tag","-z","--zero"):compare(["-c",flag])
    for flag in ("--ignore-missing","--strict","--quiet","--status","-w","--warn"):compare([flag])
    for opts in [["--tag","--text"],["--quiet","-t","--tag"],["--="],["--s"],["--t"],["--text=x"],["-?"],["--bogus"]]:
        compare(opts)
    compare(["-c"],good.replace(b"data",b"-"))
    compare(["--status","--strict","--check"],b"nonsense\n")
    Path("ok").write_bytes(good);Path("bad").write_bytes(mismatch)
    compare(["-c","-","ok"],good)
    for paths in (["ok","bad"],["bad","ok"],["ok","absent"]):compare(["-c",*paths])
    Path("data").write_bytes(bytes([corpus[0]^1])+corpus[1:])
    assert compare(["-c","ok"])[0]==1
    os.chmod("protected",0o600)
    version=run(candidate,["--version"])
    assert version[0]==0 and not version[2]
    if not os.path.samefile(candidate,GNU):
        assert version[1]==b"sha256sum (NovaOS) 0.24.0\n"
        for lang, marker in [("C",b"Compute or verify"),("fr-CA","Calcule ou vérifie".encode()),("fr-zzzzz",b"Compute or verify")]:
            os.environ["LC_ALL"]=lang
            code,stdout,stderr=run(candidate,["--help"])
            assert (code,stderr)==(0,b"") and marker in stdout
            assert all(option.encode() in stdout for option in ["--binary","--text","--check","--tag","--zero","--status","--strict","--warn","--quiet","--ignore-missing","--help","--version"])
print(f"{count} comparaisons exactes de statut, stdout et stderr ; interopérabilité GNU bidirectionnelle.")
