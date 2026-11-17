"""Rejouer les bancs depuis une copie privée, dans un nouveau dossier de résultats."""
import argparse
import os
from pathlib import Path
import random
import shutil
import subprocess
import sys
import tempfile

ROOT=Path(__file__).resolve().parents[1]
SOURCE=ROOT/"livraison-diff-0240"
QUALIFICATION=ROOT/"qualification-diff-perf-0240"


def etendre_corpus(options):
    rng=random.Random(27110)
    rows=[f"{rng.randrange(10**8):08d} série numéro {i:03d}\n".encode() for i in range(360)]
    original=b"".join(rows)
    options.DATA["fenetre-base"]=original
    variantes={"debut":"début\n".encode()+b"".join(rows[1:]),"fin":original[:-1],
               "retrait":b"".join(rows[:189]+rows[190:])}
    for name in ["disperses","ambigu"]:
        changed=rows.copy()
        for i in range(1,len(rows)-1,19):changed[i]=b"remplacement\n"
        if name=="ambigu":changed[20]=rows[1]
        variantes[name]=b"".join(changed)
    temoins=[]
    for name,contents in variantes.items():
        path="fenetre-"+name
        options.DATA[path]=contents
        for flags in ([],["-u"],["--horizon-lines=2"]):
            temoins.append(dict(option="Fenêtre / "+name,args=[*flags,"fenetre-base",path],env={},stdin=""))
    origin=options.suite
    options.suite=lambda full:origin(full)+temoins


def fichiers_p9(vecteurs):
    lignes=[f"ARC027 {i:05d} donnée native sur disque P9\n".encode() for i in range(360)]
    base=b"".join(lignes)
    files={"vecteurs":vecteurs.read_bytes(),"base":base}
    for name,indices in [("equal",[]),("start",[0]),("end",[359]),("many",range(1,360,49))]:
        right=lignes.copy()
        for i in indices:right[i]=b"MOD "+right[i]
        files["diff-"+name]=b"".join(right)
    for side in ["treeA","treeB"]:
        files[side+"/first"]=b"commun\n"
        files[side+"/part/k"]=base if side=="treeA" else files["diff-end"]
    return files


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--resultats", type=Path, default=QUALIFICATION, help="racine des nouvelles preuves")
    p.add_argument("phase",choices=["darwin","invite","synthese"])
    p.add_argument("arguments",nargs=argparse.REMAINDER)
    args=p.parse_args()
    resultats=args.resultats.resolve()
    resultats.mkdir(parents=True,exist_ok=True)
    tmp=Path(os.environ["NOVAOS_TMPDIR"])
    env={**os.environ,"GNU_DIFF":str(tmp/"cmp-0240/gnu/bin/diff"),"GNU_DIFF_SRC":str(tmp/"cmp-0240/diffutils-3.12")}
    with tempfile.TemporaryDirectory(dir=tmp,prefix="diff-banc-prive-") as td:
        work=Path(td)
        for name in ("options","flux","amont","vecteurs","synthese","invite"):
            shutil.copyfile(SOURCE/(name+".py"),work/(name+".py"))
        lab=(SOURCE/"laboratoire.py").read_text()
        assert lab.count("ROOT = HERE.parent")==1
        (work/"laboratoire.py").write_text(lab.replace("ROOT = HERE.parent","ROOT = Path("+repr(str(ROOT))+")"))
        (work/"bin").mkdir()
        shutil.copy2(QUALIFICATION/"bin/diff",work/"bin/diff")
        proofs=work/"preuves"
        if args.phase=="darwin":
            destination=resultats/"preuves-darwin"
            if destination.exists():raise FileExistsError(destination)
            driver=work/"campagne.py"
            shutil.copyfile(__file__,work/"extension.py")
            driver.write_text("import options, vecteurs\nfrom extension import etendre_corpus\n"
                              "etendre_corpus(options)\nassert options.main()==0\n"
                              "vecteurs.build(options.HERE/'preuves/vecteurs.bin')\n")
            subprocess.run([sys.executable,"-B",driver,"--complet","--sortie",proofs/"complet"],env=env,check=True)
            for script in ("flux","amont"):
                subprocess.run([sys.executable,"-B",work/(script+".py")],env=env,check=True)
            shutil.copytree(proofs,destination)
        else:
            proofs.mkdir()
            if args.phase=="invite":
                shutil.copyfile(resultats/"preuves-darwin/vecteurs.bin",proofs/"vecteurs.bin")
            else:
                shutil.copytree(resultats/"preuves-darwin/complet",proofs/"complet")
            script=work/(args.phase+".py")
            if args.phase=="invite":
                shutil.copyfile(__file__,work/"extension.py")
                script=work/"campagne.py"
                script.write_text("import invite\nfrom extension import fichiers_p9\n"
                                  "invite.inputs=lambda: fichiers_p9(invite.HERE/'preuves/vecteurs.bin')\ninvite.main()\n")
            subprocess.run([sys.executable,"-B",script,*args.arguments],env=env,check=True)
            if args.phase=="synthese":
                destination=resultats/"preuves-invites"
                destination.mkdir(exist_ok=False)
                for name in ("MATRICE.md","QEMU.md"):
                    shutil.copyfile(proofs/name,destination/name)
                shutil.copytree(proofs/"qemu",destination/"qemu")
                shutil.copytree(proofs/"complet",destination/"complet")


if __name__=="__main__":main()
