"""Rejouer env-S.pl intact depuis le checkout GNU v9.12, suite Perl amont."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
from options import GNU, NOVA, sha

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("source",type=Path)
    p.add_argument("out",type=Path)
    a=p.parse_args()
    source=a.source.resolve()
    rev=subprocess.check_output(["git","-C",source,"rev-parse","HEAD"],text=True).strip()
    assert rev=="c0f8514d989184921d9b12a4d103a7b23abc5af8"
    a.out.mkdir(parents=True,exist_ok=True)
    results={}
    for name,bin in [("GNU",GNU),("Nova",NOVA)]:
        with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"],prefix="env-perl-") as work:
            root=Path(work)
            (root/"src").mkdir()
            (root/"src/env").symlink_to(bin)
            env=dict(LC_ALL="C",PATH=f"{root}/src:/opt/homebrew/opt/coreutils/libexec/gnubin:/usr/bin:/bin",SHELL="/bin/sh",
                     TMPDIR=os.environ["TMPDIR"],srcdir=str(source),abs_top_builddir=work,VERBOSE="yes",built_programs="env")
            script=source/"tests/env/env-S.pl"
            cmd=["/usr/bin/perl","-I"+str(source/"tests"),"-MExporter","-MCuSkip","-MCoreutils",str(script)]
            result=subprocess.run(cmd,env=env,cwd=work,capture_output=True,timeout=90)
            results[name]=dict(statut=result.returncode,binaire=sha(bin),suite=sha(script),revision=rev)
            (a.out/f"amont-{name}.log").write_bytes(result.stdout+result.stderr)
            print(name,result.returncode)
    (a.out/"amont.json").write_text(json.dumps(results,indent=1)+"\n")
    return int(any(row["statut"] for row in results.values()))
if __name__=="__main__":raise SystemExit(main())
