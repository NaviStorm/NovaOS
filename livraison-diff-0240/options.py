from laboratoire import *
import itertools
import random
import struct

# Inventaire indépendant du parseur Nova. Les noms internes de GNU sont inclus.
OPTIONS="""
normal brief:q report-identical-files:s context:cC unified:uU ed:e rcs:n side-by-side:y width:W left-column suppress-common-lines
show-c-function:p show-function-line:F label:L expand-tabs:t initial-tab:T tabsize suppress-blank-empty paginate:l recursive:r no-dereference new-file:N unidirectional-new-file:P ignore-file-name-case no-ignore-file-name-case exclude:x exclude-from:X starting-file:S from-file to-file ignore-case:i ignore-tab-expansion:E ignore-trailing-space:Z ignore-space-change:b ignore-all-space:w ignore-blank-lines:B ignore-matching-lines:I text:a strip-trailing-cr ifdef:D old-group-format new-group-format unchanged-group-format changed-group-format line-format old-line-format new-line-format unchanged-line-format minimal:d horizon-lines speed-large-files:H color palette help version:v binary forward-ed:f inhibit-hunk-merge sdiff-merge-assist -no-directory -presume-output-tty
""".split()
VALUES={"context":"1","unified":"1","width":"28","show-function-line":"^func","label":"SRC","tabsize":"4","exclude":"*.bak","exclude-from":"exclude","starting-file":"m","from-file":"a","to-file":"b","ignore-matching-lines":"^#","ifdef":"FEATURE","horizon-lines":"2","color":"always","palette":"de=34:ad=35","line-format":"[%dn]%L"}
for k in ("new","old","unchanged"):
    VALUES[k+"-line-format"]="%03dn:%l%c'\\012'"
for k in ("new","old","unchanged","changed"):
    VALUES[k+"-group-format"]="%df:%dF:%(n=N?=:+)%<%>%="
DATA={"a":b"func key\nleft\n\told\n#1\nlast\n","b":b"func key\nright\n\tnew\n#2\nlast\n","-file":b"one\n","white1":b"ABC\t X  \n\n","white2":b"abc      X\n","empty":b"","dot":b".\nX\n.\n","tail":b"last","tail2":b"next","crlf":b"a\r\nb\r\n","lf":b"a\nb\n","bin1":b"a\0\xff\n","bin2":b"a\0\xfe\n","exclude":b"*.bak\npart\n","left/a.bak":b"A\n","right/a.bak":b"B\n","left/m":b"m\n","right/m":b"n\n","left/part/a":b"1\n","right/part/a":b"2\n","left/Only":b"X\n","right/only":b"Y\n","left/unique":b"X\n","right/extra":b"Y\n","protected":b"secret\n"}
STAMP=1700000000000000000
def setup(d):
    for n,b in DATA.items():
        f=d/n;f.parent.mkdir(exist_ok=True,parents=True);f.write_bytes(b);os.utime(f,ns=(STAMP,STAMP))
    for n,dest in [("syma","a"),("symb","b"),("dangling","missing")]:
        (d/n).symlink_to(dest)
    (d/"protected").chmod(0)
def suite(full):
    rows=[]
    def add(key,args,stdin=b"",env=None):
        rows.append(dict(option=key,args=args,stdin=stdin.hex(),env=env or {}))
    dirs={"recursive","ignore-file-name-case","no-ignore-file-name-case","exclude","exclude-from","starting-file","new-file","unidirectional-new-file"}
    for entry in OPTIONS:
        name,_,short=entry.partition(":")
        inputs=["left","right"] if name in dirs else ["a","b"]
        if name=="no-dereference":inputs=["syma","symb"]
        if name in ("help","version"):inputs=[]
        if name=="-no-directory":inputs=["left","right"]
        if name=="report-identical-files":inputs=["a","syma"]
        if name.startswith("ignore-") and name not in dirs:inputs=["white1","white2"]
        value=VALUES.get(name)
        spellings=[("--"+name,[("--"+name+"="+value) if value else "--"+name])]
        if name in ("context","unified","color"):spellings.append(("--"+name,["--"+name]))
        if value and name not in ("context","unified","color") and full:spellings.append(("--"+name,["--"+name,value]))
        for s in short:
            arg=value if s not in "cu" else None
            spellings.append(("-"+s,["-"+s]+([arg] if arg is not None else [])))
            if arg and full:spellings.append(("-"+s,["-"+s+arg]))
        for alias,args in spellings:
            add(alias,args+inputs)
            if full and inputs:
                combo={"left-column":["-y"],"suppress-common-lines":["-y"],"width":["-y"],"palette":["--color=always"],"show-function-line":["-u"],"-presume-output-tty":["--color"],"tabsize":["-t"],"initial-tab":["-u"],"suppress-blank-empty":["-u"],"exclude":["-r"],"exclude-from":["-r"],"ignore-case":["-wb"]}.get(name,["-a"])
                add(alias,combo+args+["--"]+inputs)
            if full and value and not (name in ("context","unified","color") and alias.startswith("--")) and alias not in ["-c","-u"]:
                add(alias,[alias])
        if full and value and name not in ("context","unified","color"):add("--"+name,["--"+name,"--","a","b"])
        if full and not value:add("--"+name,["--"+name+"=oops","a","b"])
    for s in "0123456789h":add("-"+s,["-"+s+"u","a","b"])
    for args in [[],["a"],["--","a","b"],["a","b","a"],["--","-file","b"],["-i","--","-file","b"],["a","-u","b"],["--bogus"],["-j"],["--co"],["--uni","a","b"],["-C"],["-W0"],["-U-1"],["--tabsize=0"],["--horizon-lines=-1"],["--color=yes"],["--color="],["-I","["],["-q","bin1","bin2"],["bin1","bin2"],["-a","bin1","bin2"],["-s","a","a"],["-q","empty","empty"],["-B","empty","lf"],["--strip-trailing-cr","crlf","lf"],["-N","missing","a"],["-P","a","missing"],["missing","absent"],["protected","a"],["a","protected"],["-rN","left","right"],["a","left"],["-","left"],["--from-file=a","b","a","missing"],["--from-file=a","--to-file=b"],["-","-"],["-Eib","white1","white2"],["-I","^#","-I","^x","a","b"],["--no-dereference","dangling","syma"],["-W20","-W21"],["--tabsize=2","--tabsize=3"],["-LA","-LB","-LC"],["-D","X","-D","Y"],["-C0","-U2"],["--color=always","--palette=de=35:ad=34","-u","a","b"]]:
        add("contrat",args)
    for args in [["-","a"],["a","-"],["--","-","a"],["-u","-LA","-LB","-","b"]]:add("stdin",args,DATA["a"])
    styles=["--normal","-u","-c","-e","-f","-n","-y","-DX"]
    for style in styles:
        for pair in [["empty","b"],["a","empty"],["a","dot"],["tail","tail2"],["a","a"]]:
            add("format", [style,"-LA","-LB",*pair])
    if full:
        for a,b in itertools.combinations(styles,2):add("incompatibilité",[a,b,"a","b"])
        for lang in ["fr-CA","fr-Latn-FR","fr--CA","fr-ZZ-x-","en-GB"]:
            for opt in ["--help","--version"]:add("ARC-026",[opt],env={"LANG":lang})
        for style in styles:
            for flags in [["-t"],["-T"],["--suppress-blank-empty"],["-B","-i"]]:add("combinaison",[style,*flags,"-LA","-LB","white1","white2"])
        for n in range(40):
            seed=random.Random(n)
            data=b"".join(seed.choice([b"x\n",b"y\n",b"z\n",b"same\n"]) for _ in range(8))
            add("alignement",[styles[n%8],"-LA","-LB","-","a"],data)
    return rows
def main():
    args=parse("Comparer les statuts, stdout et stderr de Nova et GNU 3.12.")
    out=(args.sortie or HERE/"preuves"/("complet" if args.complet else "rapide")).resolve();out.mkdir(parents=True,exist_ok=True)
    rows=suite(args.complet);g=oracle()
    with tempfile.TemporaryDirectory(dir=TMP,prefix="diff-options-") as tmp:
        d=Path(tmp);setup(d)
        for n,row in enumerate(rows):
            row["id"]=n
            for name,bin in [("GNU",g),("Nova",NOVA)]:
                row[name]=execute(bin,row["args"],d,bytes.fromhex(row["stdin"]),row["env"])
            doc=any(a in ("--help","--version","-v") for a in row["args"]) and row["GNU"][0]==0
            if doc:
                lang=row["env"].get("LANG","en");fr=lang in ["fr-CA","fr-Latn-FR"]
                ver=any(a in ("--version","-v") for a in row["args"])
                expected=(("diff (NovaOS) 0.24.0\n"+("Version native de NovaOS.\n" if fr else "Native NovaOS version.\n")).encode() if ver else (ROOT/"components/diff"/("diff.fr" if fr else "diff.en")).read_bytes())
                row["reference"]="ARC-026";row["ok"]=row["Nova"]==[0,expected.hex(),""]
            else:row["reference"]="GNU";row["ok"]=row["GNU"]==row["Nova"]
    save(out/"options.json",dict(environnement=environment(),cas=rows))
    groups={}
    for row in rows:groups.setdefault(row["option"],[]).append(row)
    (out/"MATRICE.md").write_text("| Option, alias ou groupe | Conformes / essais Darwin | Identifiants en écart |\n|---|---:|---|\n"+"".join(f"| {k} | {sum(x['ok'] for x in v)}/{len(v)} | {', '.join(str(x['id']) for x in v if not x['ok'])} |\n" for k,v in groups.items()))
    bad=[x["id"] for x in rows if not x["ok"]];print(len(rows),"cas,",len(bad),"écarts",bad)
    return int(bool(bad))
if __name__=="__main__":raise SystemExit(main())
