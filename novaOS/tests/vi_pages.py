"""P9 sur disque jetable : ouverture partielle, édition, suffixe valide ou refusé."""
import argparse
import json
import re
import subprocess
import tempfile
import time
from pathlib import Path
from cli import HOST, Terminal

SIZES = (1 << 20, 4 << 20, 16 << 20, 100 << 20, 1 << 30)
LINE = b"page: " + b"A" * 121 + b"\n"
# UTF-8 et CRLF coupés entre deux lectures, au milieu des données non affichées.
DENSE = LINE * 32 + b"x" * 4095 + "猫".encode() + b"\r\n" + b"a" * 4090 + b"\r\n" + LINE * 128
LARGE = LINE * 1024
# Le préfixe de vi finit à 4096. Le premier lot du service coupe le scalaire,
# et le lot suivant tombe entre CR et LF. Le dernier octet n'est pas un LF.
EDGES = LINE * 32 + b"a"*65535 + "猫".encode() + b"b"*(65536-4) + b"\r\nfin"


def fixture(root, sector):
    image = root / "p9.raw"
    commands = []
    for n in SIZES:
        commands += [f"create /f{n}", f"write /f{n} 0 {(LINE * 64).hex()}",
                     f"write /f{n} {n-1} 5a"]
    for name, data in [("dense", DENSE), ("invalide", DENSE+b"\xff"), ("vi-essai.nsh", DENSE), ("grand", LARGE), ("bords", EDGES), ("tardif", EDGES+b"\xff")]:
        commands.append(f"create /{name}")
        commands.extend(f"write /{name} {at} {data[at:at+65536].hex()}" for at in range(0,len(data),65536))
    with (root / "base.raw").open("x+b") as base, image.open("xb") as out:
        result = subprocess.run([HOST, "script", "1024", str(sector)], stdout=base,
                                input=("\n".join(commands)+"\n").encode(), stderr=subprocess.PIPE, check=True)
        ids = re.search(rb"FS=(\w+) LV=(\w+)", result.stderr).groups()
        base.seek(0)
        subprocess.run([HOST, "migrate-content-p9", "1024", str(sector), *ids],
                       stdin=base, stdout=out, check=True)
    return image, ids


def trial(arch, sector, directory, inject, rename=False):
    image, ids = fixture(directory, sector)
    rows = []
    with Terminal(arch, sector, image, "vi-pages", test=True) as t:
        t.until(b"nsh> ", 300)
        def send(keys):
            t.vm.stdin.write(keys.encode())
        def wait(s):
            return t.until(s.encode(), 180)
        def start(cmd, name):
            now = time.monotonic_ns()
            send(cmd+"\n")
            wait("\x1b[6n")
            send("\x1b[24;80R")
            data = wait(name+" [enregistré] 1:1")
            assert LINE[:56] in data
            return round((time.monotonic_ns()-now)/1e6, 3)
        for n in SIZES:
            name = f"f{n}"
            ms = start("vi "+name, name)
            send("iZ\x1b")
            wait(f"vi [NORMAL] {name} [modifié] 1:1")
            send(":q\r")
            wait("modifications non sauvegardées")
            send(":q!\r")
            wait("nsh> ")
            rows.append(dict(octets_logiques=n,premier_ecran_ms=ms))
        start("vi dense", "dense")
        send("iZ\x1b:w copie\r")
        wait("Saisir OUI puis Entrée : ")
        send("OUI\r")
        wait("fichier enregistré")
        send(":q\r")
        wait("nsh> ")
        for name, dest, valid in [("bords","bords-ok",True),("tardif","bords-non",False)]:
            start("vi "+name,name)
            send("iZ\x1b:w "+dest+"\r")
            wait("Saisir OUI puis Entrée : ")
            send("OUI\r")
            wait("fichier enregistré" if valid else "argument invalide")
            send(":q\r" if valid else ":q!\r")
            wait("nsh> ")
        start("vi invalide", "invalide")
        send("iZ\x1b:w refuse\r")
        wait("Saisir OUI puis Entrée : ")
        send("OUI\r")
        wait("argument invalide")
        send(":q\r")
        wait("modifications non sauvegardées")
        send(":q!\r")
        wait("nsh> ")
        start("vi grand", "grand")
        send("G")
        wait("grand [enregistré] 1025:1")
        send("ggo\x1bG")
        wait("grand [modifié] 1026:1")
        send("ugg")
        wait("grand [enregistré] 1:1")
        send("iZ\x1bG")
        wait("grand [modifié] 1025:1")
        send(":w depuis-fin\r")
        wait("Saisir OUI puis Entrée : ")
        send("OUI\r")
        wait("fichier enregistré")
        # Changer au-delà du trou et rejouer un groupe sans perdre le préfixe.
        send("iY\x1bu")
        wait("depuis-fin [enregistré] 1025:1")
        send("\x12:w deux-zones\r")
        wait("Saisir OUI puis Entrée : ")
        send("OUI\r")
        wait("fichier enregistré")
        send(":q\r")
        wait("nsh> ")
        if inject:
            start("@probe vi-concurrence", "vi-essai.nsh")
            send("iZ\x1bG")
            wait("conflit : la source a changé")
            send(":w refuse\r")
            wait("conflit : la source a changé")
            send(":q\r")
            wait("modifications non sauvegardées")
            send(":q!\r")
            wait("nsh> ")
            start("@probe vi-concurrence", "vi-essai.nsh")
            send("iY\x1b:w jamais\r")
            refusal=wait("conflit : la source a changé")
            assert b"Saisir OUI" not in refusal
            send(":q\r")
            wait("modifications non sauvegardées")
            send(":q!\r")
            wait("nsh> ")
        if rename:
            start("@probe vi-concurrence", "vi-essai.nsh")
            send("iK\x1b:w renom-test\r")
            assert b"Saisir OUI" not in wait("conflit : la source a changé")
            send(":q\r")
            wait("modifications non sauvegardées")
            send(":q!\r")
            wait("nsh> ")
        t.exit()

    def host(op,*args,ok=True):
        with image.open("rb") as src:
            return subprocess.run([HOST,op,"1024",str(sector),*args],stdin=src,
                                  stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=ok)
    host("verify",*ids)
    slots=re.findall(r"slot=(\d+) génération=(\d+)".encode(),host("inspect").stdout)
    slot=max(slots,key=lambda v:int(v[1]))[0]
    for name,expected in [("dense",DENSE),("copie",b"Z"+DENSE),("invalide",DENSE+b"\xff"),
                          ("depuis-fin",b"Z"+LARGE),("deux-zones",b"Z"+LARGE+b"Y"),("grand",LARGE)]:
        assert host("salvage",*ids,slot,"/"+name).stdout==expected
    assert host("salvage",*ids,slot,"/bords-ok").stdout==b"Z"+EDGES
    assert host("salvage",*ids,slot,"/tardif").stdout==EDGES+b"\xff"
    missing=host("salvage",*ids,slot,"/bords-non",ok=False)
    assert missing.returncode and b"NotFound" in missing.stderr
    missing=host("salvage",*ids,slot,"/refuse",ok=False)
    assert missing.returncode and b"NotFound" in missing.stderr, missing.stderr
    if inject:
        missing=host("salvage",*ids,slot,"/jamais",ok=False)
        assert missing.returncode and b"NotFound" in missing.stderr
        expected=bytearray(DENSE)
        expected[4096]=ord("!")
        expected[12345]=ord("?")
        assert host("salvage",*ids,slot,"/source-deplacee" if rename else "/vi-essai.nsh").stdout==expected
        if rename:
            assert host("salvage",*ids,slot,"/vi-essai.nsh").stdout==b"tiers"
            absent=host("salvage",*ids,slot,"/renom-test",ok=False)
            assert absent.returncode and b"NotFound" in absent.stderr
    raw=t.archive.read_bytes()
    pids=re.findall(rb"CLI spawn image=14 pid=(\d+)",raw)
    for row,pid in zip(rows,pids):
        values=re.findall(rb"CLI mem pid="+pid+rb" privees=(\d+) pile=(\d+)",raw)
        assert len(values)>=2
        row["resident_lancement_et_premier_ecran_kio"]=[4*(int(p)+int(s)) for p,s in values[:2]]
    return dict(architecture=arch,secteur=sector,creux=rows,publication_dense_octets=len(DENSE)+1,
                bords_64k_valides=True,erreur_apres_premier_lot=True,suffixe_invalide_refuse=True,conflit_injecte=inject,conflit_sauvegarde_sans_lecture=inject,
                suppression_refusee_et_nom_remplace=rename,index_apres_insertion_ligne=True,publication_depuis_vue_finale=len(LARGE)+1,publication_deux_zones=len(LARGE)+2)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=("aarch64","x86_64"))
    p.add_argument("rapport",type=Path)
    p.add_argument("--secteur",type=int,choices=[512,4096],default=4096)
    p.add_argument("--injection",action="store_true",help="uniquement avec le patch de concurrence")
    p.add_argument("--remplacement",action="store_true",help="sonde additionnelle io-0165-contrats.patch")
    args=p.parse_args()
    if args.remplacement and not args.injection: p.error("--injection requis")
    with tempfile.TemporaryDirectory(prefix="nova-vi-pages-") as d:
        result=trial(args.arch,args.secteur,Path(d),args.injection,args.remplacement)
    args.rapport.write_text(json.dumps(result,indent=2,ensure_ascii=False)+"\n")
