"""Compare les statuts et les deux sorties brutes, octet par octet."""
import base64
import json
import os
import random
import subprocess
import tempfile
from pathlib import Path
from laboratoire import HERE, TMP, SEED, digest, parser, tools

p = parser(__doc__)
p.add_argument("--large",action="store_true",help="ajouter un cas creux au-delà de 4 Gio")
p.add_argument("--vecteurs",type=Path,help="exporter les entrées et sorties pour le témoin SDK")
a = p.parse_args()
from vecteurs import Vectors
vectors=Vectors()
bins = tools(a)
# Une exécution d'une seule cible vérifie les résultats de référence livrés.
snapshot = HERE/"preuves/options.json"
expected = {}
if len(bins)==1 and snapshot.exists():
    expected = {r["id"]: r for r in json.loads(snapshot.read_text())["cas"]}
rows=[]
failures=[]
def case(key,argv,stdin=b"",status=None,locale="C",lang="C",doc=None,regular=False):
    observed={}
    for name,path in bins.items():
        env={**os.environ,"LC_ALL":locale,"LANG":lang}
        env.pop("POSIXLY_CORRECT",None)
        # Garder argv[0] identique pour les diagnostics getopt.
        kwargs={"cwd":work,"env":env,"capture_output":True,"timeout":10,"executable":path}
        if regular:
            with (work/"stdin-regulier").open("w+b") as fd:
                fd.write(stdin)
                fd.seek(0)
                proc=subprocess.run(["cmp",*argv],stdin=fd,**kwargs)
        else:
            proc=subprocess.run(["cmp",*argv],input=stdin,**kwargs)
        observed[name]=[proc.returncode,proc.stdout.hex(),proc.stderr.hex()]
    row=dict(id=key,args=argv,stdin=stdin.hex(),locale=locale,lang=lang,
             attendu=status,document=doc,stdin_regulier=regular,mesures=observed)
    bad=[]
    if doc:
        for name,(rc,out,err) in observed.items():
            data=bytes.fromhex(out)
            if rc!=0 or err or not data:bad.append("document")
            if name=="Nova" and doc.encode() not in data:bad.append("texte Nova")
    else:
        oracle=observed.get("GNU",expected.get(key,{}).get("mesures",{}).get("GNU"))
        for name,value in observed.items():
            if oracle and value!=oracle:bad.append("écart "+name)
            if status is not None and value[0]!=status:bad.append("vérité du corpus "+name)
    row["ecarts"]=bad
    if bad:failures.append((key,bad))
    rows.append(row)
    if a.vecteurs:vectors.append(row,work,files)
with tempfile.TemporaryDirectory(dir=TMP,prefix="cmp-options-") as tmp:
    work=Path(tmp)
    files={"a":b"x\n\0\x80\xff","b":b"x\n\x01\x7f\xfe","vide":b"",
           "-l":b"x\n\0\x80\xff","copie":b"x\n\0\x80\xff","lf":b"x\n","partiel":b"x\n\0",
           "interdit":b"", "k1":bytes(1000)+b"x"+bytes(23)+b"X",
           "k2":bytes(1024)+b"Y"}
    for n,data in files.items():(work/n).write_bytes(data)
    os.chmod(work/"interdit",0)
    os.link(work/"a",work/"lien")
    for flag in ["","-l","-s"]:
        for left in ["a","b","vide","absent"]:
            for right in ["a","b","vide","absent"]:
                rc=2 if "absent" in (left,right) else int(files[left]!=files[right])
                case(f"amont-{flag}-{left}-{right}",([flag] if flag else [])+[left,right],status=rc)
    for flag in ["-b","-c","--print-bytes","--print-chars","-l","--verbose","-bl","-lb",
                 "-cl","-bc","-s","--silent","--quiet","-bs","--prinT","--verbo","--print-b","--print-c","--q","--s"]:
        case("option-"+flag,[flag,"a","b"],status=2 if flag=="--prinT" else 1)
    for args in [[],["-s"],["--"],["--","-l","a"],["--","absent"],["a","-l","b"],
                 ["--v"],["--ve"],["--ver"],["--pr"],["--p=x"],["--oops","absent"],
                 ["-x"],["-h"],["-z"],["-ls"],["-sl"],["--silent","--verbose"],["-i"],
                 ["--ig"],["-n"],["--by"],["--sil=0"],["--help=0"],
                 ["a","b","0","0","plus"],[".","a"],["-s",".","a"],["interdit","b"],
                 ["absent","absent"],["-n0","absent","b"],["-s","absent","b"],
                 ["a","copie"],["a","lien"],["a"],["-","a"],["a","-"],["-","-"],
                 ["-i1:2","-","-"],["-i1:2","a","a"],["-i","99","a","vide"],
                 ["-i","1:2","-n0","a","b"],["-i","1:2","a","b","3","1"],
                 ["-i2:0","-i1","a","b"],["a","b","2","3"],["--bytes=2","a","b"],
                 ["--ignore-initial","2:3","a","b"],["-i1:0","-","a"],["-l","a","-"],
                 ["-i","1000000000000000000000000","-","b"],["-n4","-n2","a","b"],
                 ["-n2","-n4","a","b"],["-bl","lf","a"],["partiel","a"],["lf","a"],
                 ["vide","a"],["a","vide"],["-s","lf","a"],["-l","lf","a"]]:
        case("argv-"+repr(args),args,files["a"])
    for args in [["-","-","1","2"],["-","-","1","0"],["-n0","-","-","1","2"],["-s","-","-","0","1"],["-l","a","-"]]:
        case("stdin-regulier-"+repr(args),args,files["a"],regular=True)
    for args in [["--=x"],["-n0",".","b"],["--bad","interdit"],["-n","x'y"],["-i","x\\ny"],["--","abc@host"],["--","fichier avec espace"]]:
        case("diagnostic-"+repr(args),args)
    for val in ["0","1","2","3","010","0x10","0X02","0b10","08","-1","-0","+3"," 2",
                "K","k","kB","KB","KD","KiB","1K","1KiB","1kD","1MB","1MiB","0G","1T",
                "1P","1E","1Z","1Y","1Q","1R","1b","1kib","1m","1 ","","0x","1:0",
                "0:1",":2","2:","1::1","4294967297","9223372036854775807",
                "9223372036854775808","99999999999999999999999999"]:
        for opt in ["-n","--bytes=","-i","--ignore-initial="]:
            args=([opt,val] if opt.startswith("-") and not opt.startswith("--") else [opt+val])
            case("nombre-"+repr(args),args+["k1","k2"])
    for locale in ["C","en_US.UTF-8","fr_FR.UTF-8"]:
        for flag in ["-b","-bl","-l","-s","-i2","-n3"]:
            case(locale+flag,[flag,"a","b"],locale=locale)
    for lang,marker in [("en","Compare two inputs"),("fr-CA","Comparer deux entrées"),
                         ("FR-Latn-FR","Comparer deux entrées"),("fr_XX","Compare two inputs"),
                         ("fr-zzzz-invalid","Compare two inputs")]:
        for flag in ["--help","--he"]:
            case(lang+flag,[flag],lang=lang,doc=marker)
        for flag in ["-v","--version","--versi"]:
            case(lang+flag,[flag],lang=lang,doc="cmp (NovaOS)")
    rng=random.Random(SEED)
    for i in range(96):
        n=rng.randrange(2,1100)
        left=rng.randbytes(n)
        right=bytearray(left)
        for k in range(i%5):right[rng.randrange(n)]^=1
        if i%7==0:del right[-1:]
        (work/"g").write_bytes(left);(work/"h").write_bytes(right)
        skip=rng.randrange(4);limit=rng.randrange(n+1)
        flag=["-b","-l","-bl","-s"][i%4]
        case(f"graine-{i}",[flag,f"-i{skip}",f"-n{limit}","g","h"],
             status=int(left[skip:skip+limit]!=right[skip:skip+limit]))
    # Cas où le statut GNU peut contredire les octets affichés par -l.
    for n in ["grand1","grand2"]:(work/n).write_bytes(bytes(256*1024))
    with (work/"grand2").open("r+b") as f:f.write(b"X")
    case("verbose-ecart-premier-bloc",["-l","grand1","grand2"],status=1)
    if a.large:
        for n in ["sparse1","sparse2"]:
            with (work/n).open("wb") as f:f.seek(2**32+19);f.write(n[-1].encode())
        case("position-64-bits",["-i4294967296","sparse1","sparse2"],status=1)
if a.vecteurs:
    vectors.save(a.vecteurs)
    a.vecteurs.with_suffix(".json").write_text(json.dumps(vectors.ids,ensure_ascii=False)+"\n")
report=dict(graine=SEED,binaires={k:dict(path=str(v),sha256=digest(v)) for k,v in bins.items()},cas=rows)
if a.json:a.json.write_text(json.dumps(report,ensure_ascii=False,indent=1)+"\n")
print(len(rows),"cas,",len(failures),"avec écart", failures[:20])
raise SystemExit(bool(failures))
