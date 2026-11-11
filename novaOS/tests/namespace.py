"""Images autonomes, contrôle des deux ancres sur macOS et redémarrages indépendants."""
import argparse
import os
from pathlib import Path
import re
import subprocess
import tempfile

from bloc import Bench
from fichiers import HOST


def outil(b, op, *args):
    with b.image.open("rb") as image:
        return subprocess.check_output([HOST, op, "512", str(b.sector), *args], stdin=image)


def generation(b):
    records = re.findall(r"slot=(\d+) génération=(\d+) blocs=\d+ FS=(\w+) LV=(\w+)".encode(), outil(b, "inspect"))
    assert len(records) == 2
    slot, gen, fs, lv = max(records, key=lambda row: int(row[1]))
    outil(b, "verify", fs, lv)
    return int(gen), fs, lv, slot


def image_initiale(b):
    path = "/a"
    cmds = ["mkdir /a", "mkdir /public", "create /public/marque", "write /public/marque 0 7075626c6963"]
    for ch in "bcd":
        path += "/" + ch * 250
        cmds.append("mkdir " + path)
    file = path + "/" + "e" * 64
    assert len(file) == 820 and len("/" + "z"*255 + file[2:]) == 1074
    cmds += ["create " + file, "write " + file + " 0 4e6f7661"]
    with b.image.open("wb") as image:
        subprocess.run([HOST,"script","512",str(b.sector)],input=("\n".join(cmds)+"\n").encode(),stdout=image,check=True)
    return generation(b)[0]


def hote(b, expected):
    # Le Mach-O opposé à l'architecture invitée relit directement les arêtes.
    arch = "x86_64" if b.arch == "aarch64" else "aarch64"
    target = arch + "-apple-darwin"
    binary = b.tmp / "lecteur"
    if not binary.exists():
        fs = HOST.parents[3]
        subprocess.run([fs/"lab/run","build","--offline","--lib"],
                       env={**os.environ,"NOVAFS_TARGET":target},check=True)
        lib = fs/"target"/target/"release"
        subprocess.run(["rustup","run","stable","rustc","--edition=2024","--target",target,
                        "-C","opt-level=2","-L","dependency="+str(lib/"deps"),
                        "--extern","novafs="+str(lib/"libnovafs.rlib"),
                        Path(__file__).with_name("namespace_oracle.rs"),"-o",binary],check=True)
    return tuple(map(int, subprocess.check_output([binary,b.image,str(b.sector),expected]).split()))


def succes(serie):
    assert "BLOC session terminee ok=true" in serie, serie[-1600:]


def normal(b):
    old = image_initiale(b)
    before_gen, oid = hote(b,"ancien")
    assert before_gen == old
    s = b.run("namespace-write")
    succes(s)
    for marque in ["NOMS seconde session RO cwd=/public", "NOMS mutations et refus OK", "NOMS lecture par identite OK"]:
        assert marque in s, s[-1500:]
    gen, fs, lv, slot = generation(b)
    assert gen > old and hote(b,"nouveau") == (gen,oid)
    assert outil(b,"salvage",fs,lv,slot,"/public/marque") == b"public"
    before = b.image.read_bytes()
    s = b.run("namespace-read", readonly=True)
    succes(s)
    assert "NOMS lecture par identite OK" in s and "NOMS seconde session RO cwd=/public" in s
    assert b.image.read_bytes() == before
    s = b.run("namespace-disconnect",readonly=True)
    succes(s)
    assert "NOMS fin du second client Exit(9)" in s and "NOMS survivant apres depart du pair" in s
    assert b.image.read_bytes() == before
    b.check("deux clients, mutations et refus, identité conservée à 1 074 octets après redémarrage")


def depth(b):
    x, z, y, v = (ch*n for ch,n in [("x",255),("z",255),("y",254),("v",254)])
    cmds = ["mkdir /t"]
    def file(path):
        cmds.extend(["create "+path, "write "+path+" 0 4e6f7661"])
    # Assemblage de bas en haut avec la CLI historique et des opérandes courts.
    for level in range(31,0,-1):
        cmds.extend(["mkdir /n","rename /t /n/"+x])
        if level in (3,15,31):
            file("/n/"+z)
        if level == 3:
            file("/n/"+y)
            cmds.append("mkdir /n/"+v)
            file("/n/"+v+"/k")
        cmds.append("rename /n /t")
    cmds.append("rename /t /"+x)
    with b.image.open("wb") as image:
        subprocess.run([HOST,"script","512",str(b.sector)],input=("\n".join(cmds)+"\n").encode(),stdout=image,check=True)
    gen = generation(b)[0]
    s = b.run("namespace-depth",timeout=240)
    succes(s)
    assert s.count("NOMS profondeurs et longueurs OK") == 2
    assert "STOCKAGE restitution apres deux sessions=true" in s
    assert generation(b)[0] == gen+10
    b.check("chemins de 1023, 1024, 1025, 4096 et 8192 octets, refus au niveau 33")


def recovery(b):
    initial = image_initiale(b)
    old_generation, oid = hote(b,"ancien")
    assert initial == old_generation
    base = b.image.read_bytes()
    def recover(label, allowed):
        gen = generation(b)[0]
        state = {initial:"ancien",initial+1:"nouveau"}[gen]
        assert state in allowed and hote(b,state) == (gen,oid)
        frozen = b.image.read_bytes()
        log = b.run("namespace-inspect",label+"-ro",readonly=True)
        succes(log)
        assert "NOMS oracle "+state in log and frozen == b.image.read_bytes()
        b.check(label+" G"+str(gen)+" "+state)
    for case, point, allowed in [
        ("namespace-f1-before","avant F1",["ancien"]),
        ("namespace-f1-inflight","armer blkdebug",["ancien"]),
        ("namespace-f1-after","apres F1",["ancien"]),
        ("namespace-f2-before","avant F2",["ancien","nouveau"]),
        ("namespace-f2-inflight","armer blkdebug",["ancien","nouveau"]),
        ("namespace-f2-after","apres F2",["nouveau"]),
        ("namespace-rename","NOMS renommage confirme",["nouveau"])]:
        b.image.write_bytes(base)
        b.run(case,cut=point if case=="namespace-rename" else "COUPURE "+point, debug="" if "inflight" in case else None)
        recover(case,allowed)
    for label,event,state,code,allowed in [
            ("write","write_aio",1,26,["ancien"]),
            ("f1","flush_to_disk",2,26,["ancien"]),
            ("f2","flush_to_disk",3,22,["ancien","nouveau"])]:
        b.image.write_bytes(base)
        cfg = b.tmp/"eio"
        rules = '[set-state]\nevent = "write_aio"\nstate = "1"\nnew_state = "2"\n' if state >= 2 else ""
        if state == 3:
            rules += '[set-state]\nevent = "flush_to_disk"\nstate = "2"\nnew_state = "3"\n'
        cfg.write_text(rules+f'[inject-error]\nevent = "{event}"\nstate = "{state}"\nerrno = "5"\n')
        s = b.run("namespace-rename","nom-eio-"+label,debug=cfg)
        assert f"client=Exit({code})" in s and "NOMS renommage confirme" not in s
        recover("nom-eio-"+label,allowed)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--sector",type=int,choices=(512,4096))
    p.add_argument("--only", choices=("normal","depth","recovery"))
    args = p.parse_args()
    for sector in ([args.sector] if args.sector else [512,4096]):
        with tempfile.TemporaryDirectory(prefix="nova-namespace-") as tmp:
            bench = Bench(Path(tmp),os.environ["NOVA_ARCH"],sector)
            for suite in (normal,depth,recovery):
                if args.only in (None,suite.__name__):
                    suite(bench)
