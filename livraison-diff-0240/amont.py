"""Rejouer la suite amont dans deux répertoires de construction privés."""
from laboratoire import *
import re
import sys
hint=TMP/"diff-gnu-3.12/source-dir"
src=Path(os.environ.get("GNU_DIFF_SRC",hint.read_text().strip() if hint.is_file() else TMP/"cmp-0240/diffutils-3.12")).resolve()
tests=re.search(r"TESTS = (.*?)\n\n",(src/"tests/Makefile.am").read_text(),re.S)[1].replace("\\"," ").split()
out=HERE/"preuves/amont";out.mkdir(parents=True,exist_ok=True)
for name,binary in [("GNU",oracle()),("Nova",NOVA)]:
    rows={}
    with tempfile.TemporaryDirectory(dir=TMP,prefix="diff-amont-") as td:
        base=Path(td);cwd=base/"tests";cwd.mkdir();bins=base/"src";bins.mkdir()
        for exe in ["diff","cmp","diff3","sdiff"]:
            (bins/exe).symlink_to(binary if exe=="diff" else oracle().parent/exe)
        env={**ENV,"srcdir":str(src/"tests"),"abs_srcdir":str(src/"tests"),"top_srcdir":str(src),"abs_top_srcdir":str(src),"abs_top_builddir":str(base),"built_programs":"diff","VERSION":"3.12","PACKAGE_VERSION":"3.12","CC":"cc","AWK":"awk","PERL":"perl","PATH":str(bins)+":"+ENV["PATH"]}
        for t in tests:
            if t in ("cmp","diff3"):continue
            try:
                p=subprocess.run(["sh","-c",'exec 9>&2; exec sh "$1"',"sh",src/"tests"/t],cwd=cwd,env=env,capture_output=True,timeout=40)
                rows[t]=[p.returncode,p.stdout.decode(errors="replace"),p.stderr.decode(errors="replace")]
            except subprocess.TimeoutExpired:rows[t]=[124,"","délai 40 s"]
    save(out/(name+".json"),rows);print(name,[(k,v[0]) for k,v in rows.items() if v[0]])

# Le scénario XFAIL décrit un alignement sous-optimal. Comparer aussi les
# octets effectivement choisis par GNU, y compris en recherche minimale.
align=[]
for mode in [[],["-d"],["-H"]]:
    args=["-u","-La","-Lb",*mode,"large-subopt.in1","large-subopt.in2"]
    got={k:execute(binary,args,src/"tests") for k,binary in [("GNU",oracle()),("Nova",NOVA)]}
    align.append(dict(args=args,ok=got["GNU"]==got["Nova"],resultats={k:[v[0],len(v[1])//2,hashlib.sha256(bytes.fromhex(v[1])).hexdigest(),v[2]] for k,v in got.items()}))
save(out/"alignement.json",align)
assert all(r["ok"] for r in align)
