"""Sous-ensemble non destructif des scripts amont, avec contrôle positif GNU sur Darwin."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from options import ROOT, GNU

p=argparse.ArgumentParser()
p.add_argument("source",type=Path)
p.add_argument("--output",type=Path,required=True)
p.add_argument("--candidate",type=Path,default=ROOT/"artifacts/darwin-arm64/sort")
args=p.parse_args()
up=args.source.resolve()
rev="106e9b2384d08a1696fcbd40cbab52237943f208"
suites=["sort-debug-keys","sort-debug-warn","sort-float","sort-month","sort-version",
        "sort-unique","sort-u-FMR","sort-files0-from","sort-NaN-infloop","sort-field-limit",
        "sort-discrim","sort-buffer-size","sort-rand","sort-h-thousands-sep","sort-locale",
        "sort-merge-fdlimit","sort-compress","sort-unique-segv"]
report={}
with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as temp:
    root=Path(temp); source=root/"source"; (source/"tests").mkdir(parents=True)
    (source/"tests"/"init.sh").write_bytes(subprocess.check_output(["git","-C",up/"gnulib","show",rev+":tests/init.sh"]))
    shutil.copy2(up/"init.cfg",source)
    subprocess.run(["cc","-std=c11","-o",root/"getlimits",Path(__file__).with_name("limites_hote.c")],check=True)
    programs = [item.name for item in GNU.parent.iterdir() if item.is_file()]

    for engine, binary in [("gnu",GNU),("nova",args.candidate.resolve())]:
        build=root/engine
        (build/"src").mkdir(parents=True)
        (build/"src/sort").symlink_to(binary)
        (build/"src/getlimits").symlink_to(root/"getlimits")
        env={**os.environ,"srcdir":str(source),"abs_top_srcdir":str(up),"abs_srcdir":str(up),
             "abs_top_builddir":str(build),"built_programs":" ".join(programs),"LC_ALL":"C",
             "PATH":str(build/"src")+":"+str(GNU.parent)+":"+os.environ["PATH"],
             "LOCALE_FR_UTF8":"fr_FR.UTF-8","LOCALE_EN_UTF8":"en_US.UTF-8","SHELL":"/bin/sh"}
        for name in suites:
            script=up/"tests/sort"/(name+".sh")
            if not script.is_file(): continue
            try:
                proc=subprocess.run(["/bin/sh",script],cwd=build,env=env,
                                    capture_output=True,timeout=35)
                status=proc.returncode
                log=(proc.stdout+proc.stderr).decode(errors="backslashreplace")
            except subprocess.TimeoutExpired:
                status,log=124,"délai du banc de 35 s"
            report.setdefault(name,{})[engine]={"status":status,"log":log}
            args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n")
