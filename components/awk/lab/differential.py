"""Comparaison en répertoires jetables isolés. stderr est comparé octet par octet."""
import argparse
import hashlib
import json
import os
import random
import subprocess
import tempfile
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def probe(binary, args, stdin, files, locale):
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp:
        work = Path(tmp)
        for name, data in files.items():
            (work/name).write_bytes(data)
        p = subprocess.run([binary, *args], input=stdin, capture_output=True, cwd=work,
                           env={**os.environ, "LC_ALL":locale, "TZ":"UTC", "AWK_TEST":"42"},
                           timeout=15)
        def stream(data):
            return {"size":len(data), "sha256":sha(data), "prefix":data[:160].hex()}
        return dict(status=p.returncode, stdout=stream(p.stdout), stderr=stream(p.stderr),
                    files={f.name:sha(f.read_bytes()) for f in work.iterdir()})


def cases():
    data = b"alpha 2\nbeta 3\nalpha 1\n"
    files = {"in":data}
    for name, args, stdin, fs in [
        ("champ", ["{print $1}","in"], b"", files),
        ("comptes", ["-F:","{print $1}","in"], b"", {"in":b"a:b\nc:d\n"}),
        ("addition",["BEGIN{print 2+3}"],b"",{}),
        ("somme",["{s+=$1} END{print s}","in"],b"",{"in":b"2\n-3\n5\n"}),
        ("intervalle",["NR>=10 && NR<=20 {print}"],b"ligne\n"*24,{}),
        ("filtre",["/erreur/ {print NR,$0}"],b"ok\nerreur alerte\n",{}),
        ("tableau",['{a[$1]++} END{for(k in a) print k,a[k]}'],b"x\nx\n",{}),
        ("stdin",["{print $1}"],data,{}),
        ("vide",["END{print NR,NF}"],b"",{}),
        ("fin-sans-lf",["{print NR,length($0)}"],b"abc",{}),
        ("longue",["{print length($0), substr($0,length($0),1)}"],b"x"*(2**20)+b"z",{}),
        ("frontiere",['BEGIN{RS=":"} {print length($0)}'],b"x"*8192+b":y:",{}),
        ("paragraphe",['BEGIN{RS=""}{print "[" $0 "]"}'],b"\nabc\n  \nx\n\nz\n",{}),
        ("recursion",["function f(n){if(n==0)return 1;return n*f(n-1)} BEGIN{print f(6)}"],b"",{}),
        ("plage",["/beta/,/alpha/"],data,{}),
        ("env",['BEGIN{print ENVIRON["AWK_TEST"],ARGV[1],ARGC} {print FILENAME, FNR,NR}',"in"],b"",files),
        ("format",['BEGIN{OFS=":";ORS="!";print sprintf("%.2f",1.5),substr("abc",2)}'],b"",{}),
        ("phases",["BEGINFILE{print FILENAME} {print} ENDFILE{print FNR}","in","empty"],b"",{**files,"empty":b""}),
        ("largeurs",['BEGIN{FIELDWIDTHS="2 1"}{print $1,$2}'],b"aab\n",{}),
        ("fpat",['BEGIN{FPAT="[0-9]+"}{print NF,$2}'],b"a12b34c\n",{}),
        ("patsplit",['BEGIN{print patsplit("a12b34",a,/[0-9]+/),a[2]}'],b"",{}),
        ("sous-tableau",['BEGIN{a[1][2]=3; print a[1][2]}'],b"",{}),
        ("fixe-multi",['BEGIN{a[1,2]=3;print a[1,2]}'],b"",{}),
        ("regex-longue",['BEGIN{match("ab",/a|ab/);print RSTART,RLENGTH}'],b"",{}),
        ("rs-regex",['BEGIN{RS=":+"}{print $0}'],b"a::b",{}),
        ("gensub",['BEGIN{print gensub(/a/,"A","g","aba")}'],b"",{}),
        ("ignorecase",['BEGIN{IGNORECASE=1}/abc/{print}'],b"ABC\n",{}),
        ("octets",["-b","{print length($0),$0}"],b"\x00\xff\xfe\n",{}),
        ("crlf",["{print length($0)}"],b"x\r\n",{}),
        ("boucles",["BEGIN{for(i=0;i<3;i++){if(i==1)continue; print i}; do{i--}while(i>0);print i}"],b"",{}),
        ("next",["{if(NR==1)next; print NR}"],data,{}),
        ("nextfile",["{print FILENAME;nextfile}","in","in"],b"",files),
        ("exit-end",['BEGIN{exit 4} END{print "fin"}'],b"",{}),
        ("fichier-prog",["-f","script","in"],b"",{**files,"script":b"{print $2}\n"}),
        ("sources",["-e",'BEGIN{print 1}',"-e",'END{print 2}',"--","in"],b"",files),
        ("affectation",["{print x,$1}","x=4","in"],b"",files),
        ("getline-fichier",['BEGIN{r=(getline a < "in");print r,a,close("in")}'],b"",files),
        ("getline-begin",['BEGIN{print (getline),NR,$0} {print $0}'],data,{}),
        ("redirect",['BEGIN{print "a">"out";close("out");printf "b">>"out";print close("out")}'],b"",{}),
        ("pipe-lecture",['BEGIN{c="printf ok";c | getline a;print a,close(c)}'],b"",{}),
        ("pipe-sortie",['BEGIN{c="cat > out";print "ok" | c;print close(c)}'],b"",{}),
        ("system",['BEGIN{print system("true")}'],b"",{}),
        ("invalide",["BEGIN {"],b"",{}),
        ("refus-sandbox",["-S",'BEGIN{print "a">"out"}'],b"",{}),
        ("refus-system",["-S",'BEGIN{system("touch out")}'],b"",{}),
        ("refus-pipe",["-S",'BEGIN{print "x" | "cat > out"}'],b"",{}),
        ("refus-getline",["-S",'BEGIN{getline x < "in"}'],b"",files),
        ("chargement",["-l","extension-inexistante","BEGIN{}"],b"",{}),
    ]:
        yield name,args,stdin,fs,"C"
    for name, args, data, files in [
        ("booleen", ['{print ($1 ? "vrai":"faux"), ("0" ? 1:0)}'], b"0\n00\n1", {}),
        ("compteurs-modifiables", ["{print NR,FNR; NR=20;FNR=-2}"], b"a\nb", {}),
        ("champs-vides", ['BEGIN{FS=""}{print NF,$1,$2}'], b"xy\n", {}),
        ("champs-blancs", ["{print NF,length($1)}"], b"a\x0bb c\n", {}),
        ("split-imbrique", ['BEGIN{a[1]=8; print split("un deux",a[2]),a[1],a[2][2]}'], b"", {}),
        ("split-vide", ['BEGIN{print split("",a,":")}'], b"", {}),
        ("casse-octets", ["{print toupper($0)}"], b"x\xe9\n", {}),
        ("source-incluse", ["-i", "./lib.awk", "-i", "./lib.awk", "BEGIN{print f()}"], b"", {"lib.awk":b"function f(){return 7}\n"}),
        ("fichier-admis-sandbox", ["-S","{print}", "in"], b"", {"in":b"ok\n"}),
        ("ere-invalide", ['BEGIN{FS="[("}{print NF}'], b"a\n", {}),
        ("fin-apres-ligne-longue", ["{print NR,length($0)}"], b"a"*2097152+b"\nx", {}),
    ]:
        yield name,args,data,files,"C"
    yield "utf8",["{print length($0),substr($0,2,1)}"],"éλ\n".encode(),{},"en_US.UTF-8"
    yield "separateur-entre-blocs",['BEGIN{RS="§"}{print length($0)}'],b"x"*8191+"§y".encode(),{},"en_US.UTF-8"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("reference", type=Path)
    parser.add_argument("awk",type=Path)
    parser.add_argument("--report",required=True,type=Path)
    args=parser.parse_args()
    binaries=[str(p.resolve(strict=True)) for p in (args.reference,args.awk)]
    jobs=list(cases())
    rng=random.Random(27)
    # Programmes synthétiques à grammaire bornée, avec et sans erreur syntaxique.
    def expr(depth):
        if depth==0:return str(rng.randrange(10))
        return "("+expr(depth-1)+rng.choice(["+","-","*","<","!="])+expr(depth-1)+")"
    for i in range(128):
        code="BEGIN { print "+expr(rng.randrange(1,5))+(" }" if i%8 else "")
        jobs.append((f"gen-{i:03}",[code],b"",{},"C"))
    results=[]
    for name,argv,stdin,files,locale in jobs:
        oracle,observed=[probe(b,argv,stdin,files,locale) for b in binaries]
        diff=[key for key in oracle if oracle[key]!=observed[key]]
        results.append(dict(name=name,args=argv,locale=locale,stdin_sha256=sha(stdin),
                            oracle=oracle,observed=observed,differences=diff))
    args.report.write_text(json.dumps(dict(seed=27,binaries={b:sha(Path(b).read_bytes()) for b in binaries},
                                           cases=results),indent=1)+"\n")
    for r in results:
        if r["differences"] and not r["name"].startswith("gen-"):
            print(r["name"],r["differences"])
    print(len(jobs), "cas,", sum(not r["differences"] for r in results),"identiques")
    raise SystemExit(int(any(r["differences"] for r in results)))


if __name__=="__main__":
    main()
