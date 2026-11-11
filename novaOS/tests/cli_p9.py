"""CLI P9 réelle, horloge hôte indépendante et arrêt forcé entre deux lancements."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

from bloc import Bench
from cli import ROOT, HOST, Terminal
from session_cli import LONG

def programme_oracle(arch, tmp):
    target = ("x86_64" if arch == "aarch64" else "aarch64") + "-apple-darwin"
    fs = ROOT.parent / "novaFS"
    subprocess.run([fs / "lab/run", "build", "--offline", "--lib"], check=True,
                   env=dict(os.environ, NOVAFS_TARGET=target))
    lib = fs / "target" / target / "release"
    binary = tmp / "oracle"
    subprocess.run(["rustup","run","stable","rustc","--edition=2024","--target",target,
                    "-L","dependency="+str(lib / "deps"),"--extern","novafs="+str(lib / "libnovafs.rlib"),
                    Path(__file__).with_name("cli_temps_oracle.rs"),"-o",binary], check=True)
    return binary

def check(arch, sector, tmp, oracle):
    image = tmp / "p9.raw"
    old = tmp / "ancienne.raw"
    large = ("écran\t界 mot\n" * 4500 + "dernière").encode()
    lines = b"".join((f"L{i:02d}\n".encode() for i in range(1,36)))
    files = {"/grand":large, "/texte":lines, "/documents/exemple.txt":b"NovaOS\n",
             "/utf8":LONG, "/sansfin":b"a\nb\nc", "/.cache":b"secret", "/-nom":b"tiret"}
    # Référence indépendante pour le comptage Unicode et les plages de lignes.
    env=dict(os.environ,LC_ALL="en_US.UTF-8")
    gnu_wc=subprocess.check_output(["gwc","-lwcmL"],input=large,env=env).split()
    assert list(map(int,gnu_wc))==[4500,13501,len(large.decode()),len(large),14]
    first20 = subprocess.check_output(["ghead", "--lines=20"], input=lines)
    assert first20 == b"".join(lines.splitlines(True)[:20])
    last20 = subprocess.check_output(["gtail", "-n20"], input=lines)
    assert last20 == b"".join(lines.splitlines(True)[-20:])
    script=["mkdir /documents"]
    for name, data in files.items():
        script.append(f"create {name}")
        for off in range(0,len(data),4096):
            script.append(f"write {name} {off} {data[off:off+4096].hex()}")
    with old.open("xb") as disk:
        setup=subprocess.run([HOST,"script","512",str(sector)],input=("\n".join(script)+"\n").encode(),
                             stdout=disk,stderr=subprocess.PIPE,check=True)
    fs,lv=re.search(rb"FS=(\w+) LV=(\w+)",setup.stderr).groups()
    old_hash=hashlib.sha256(old.read_bytes()).digest()
    # Même service et même CLI sur le volume historique, avant la migration.
    with Terminal(arch, sector, old, "profil-historique", test=True) as terminal:
        terminal.until(b"nova> ")
        terminal.line("ls -l documents", has=["mtime=inconnue"], status="sortie 0")
        terminal.line("ls -t documents", has=["indisponible"], status="sortie 25")
        terminal.line("touch -- -nom", has=["indisponible"], status="sortie 25")
    assert hashlib.sha256(old.read_bytes()).digest() == old_hash
    with old.open("rb") as src,image.open("xb") as dst:
        subprocess.run([HOST,"migrate-content-p9","512",str(sector),fs,lv],stdin=src,stdout=dst,check=True)
    assert hashlib.sha256(old.read_bytes()).digest()==old_hash
    # Le profil de production monté en lecture seule doit identifier le même LV.
    diagnostic = Bench(tmp, arch, sector)
    diagnostic.image = image
    snapshot = hashlib.sha256(image.read_bytes()).digest()
    report = diagnostic.run("files-info", "info-p9", readonly=True)
    assert f"Volume {lv.decode()}" in report and "BLOC session terminee ok=true" in report
    assert hashlib.sha256(image.read_bytes()).digest() == snapshot
    def host(op,*args):
        return subprocess.check_output([oracle,op,image,str(sector),*args])
    host("initialiser")
    alpha=host("stat","/alpha").split()
    times={}
    memory={}
    # GNU doit distinguer les deux fractions UTC, y compris dans la même seconde.
    for path,nanos in [("alpha",800_000_001),("zeta",200_000_123)]:
        local=tmp/path
        local.touch()
        stamp=978307200_000_000_000+nanos
        os.utime(local,ns=(stamp,stamp))
    names=subprocess.check_output(["gls","-1tr","--","alpha","zeta"],cwd=tmp,env=env)
    assert names.splitlines()==[b"zeta",b"alpha"]
    start=int(time.time())-2
    with Terminal(arch,sector,image,"p9-commandes",test=True) as t:
        t.until(b"BLOC chaine novaFS=")
        begin=time.monotonic()
        t.until(b"nova> ")
        times["publication_invite"]=(time.monotonic()-begin)*1000
        def run(cmd, has=(), status=0, absent=()):
            times[str(cmd)]=t.line(cmd,status=(f"sortie {status}" if status is not None else None),
                                    has=has,absent=absent)
        run("ls -latr --color /",["2001-01-01 00:00:00.200000123Z","2001-01-01 00:00:00.800000001Z","inconnue",".cache",b"\x1b[34m"])
        # Ordre exact de deux dates qui ne diffèrent que par leur fraction.
        run("ls -tr --color=never -- zeta alpha",[names.replace(b"\n",b"\r\n")],absent=[b"\x1b[34m"])
        run("ls -tr --color=always -- alpha zeta",[b"zeta\r\nalpha\r\n"])
        run("ls -tr --color=never -- -nom alpha zeta", [b"zeta\r\nalpha\r\n-nom\r\n"])
        run("ls -lAhSF --color=auto",[b"\x1b[34mdocuments\x1b[0m/"])
        run("ls -1dR -- documents",[b"documents\r\n"])
        run("ls -R",["exemple.txt"])
        run("ls -Z",["argument invalide"],3)
        run("touch vide")
        run("tail -n0 vide",absent=[b"introuvable"])
        run("touch -c absent")
        run("cat absent",["introuvable"],20)
        run("touch -a chrono-a")
        run("touch -m chrono-m")
        run("touch -a /")
        run("touch -m documents")
        run("touch -am -- -nom")
        run("cat -- -nom",["tiret"])
        run("cp grand copie")
        run("cp -n texte copie")
        run("cp texte copie",["existe déjà"],27)
        run("cp grand grand",["argument invalide"],3)
        for cmd,answer,code in [(b"cp -i texte copie", b"n", 0),
                                (b"mv -i texte copie", b"o", 27)]:
            t.vm.stdin.write(cmd+b"\n")
            t.until(b"[o/N] ")
            t.vm.stdin.write(answer+b"\n")
            assert f"[statut sortie {code}]".encode() in t.until(b"nova> ")
        run("cp -R documents double")
        run("cp -r documents documents",status=3)
        run("cat double/exemple.txt",["NovaOS"])
        run("mkdir documents/interne")
        run("mv documents documents/interne",status=3)
        run("ls documents/interne")
        run("cp documents impossible",status=30)
        run("touch ../interdit",status=5)
        run("touch -t 2000 fichier",status=3)
        run("mv alpha renomme")
        run("mv renomme documents")
        run("mv -n texte grand")
        run("head texte",[b"L10\r\n"],absent=[b"L11\r\n"])
        run("head --lines=20 texte", [first20.replace(b"\n", b"\r\n")], absent=[b"L21\r\n"])
        run("head -qc2 -- sansfin",[b"a\r\n"])
        run("tail texte",[b"L26\r\nL27"],absent=[b"L25\r\n"])
        run("tail -n20 texte", [last20.replace(b"\n", b"\r\n")], absent=[b"L15\r\n"])
        run("tail --lines=0 texte",absent=[b"L35\r\n"])
        run("tail -c1 sansfin",[b"cPROC pid="])
        run("tail -n2 sansfin",[b"b\r\ncPROC pid="])
        run("wc -L grand",["14 grand"])
        run("tail -n1 grand",["dernière"])
        run("head -n1 utf8",["é\r\n"])
        run("wc -cwmlL grand",[f"4500 13501 {len(large.decode())} {len(large)} 14 grand"])
        run("wc -l texte sansfin",["35 texte","2 sansfin","37 total"])
        run("more documents/exemple.txt",["NovaOS"])
        for key,expected in [(b" ","L35"),(b"\n","L25"),(b"q",None)]:
            t.vm.stdin.write(b"more texte\n")
            t.until(b"--More--")
            t.vm.stdin.write(key)
            if expected:
                t.until(expected.encode())
            if key==b"\n":
                t.until(b"--More--")
                t.vm.stdin.write(b"q")
            t.until(b"nova> ")
        for name in ("pwd","ls","cat","head","tail","touch","cp","mv","wc","more","mkdir","rm"):
            run(name+" --help",[name])
            run(name+" --version",["NovaOS "])
        run("alias l='ls -latr --color'",status=None)
        run("l",["inconnue", "documents", b"\x1b[34m"])
        run("l documents",["exemple.txt","renomme"])
        run("unalias l",status=None)
        run("l",["commande inconnue"],None)
        run("alias a=b",status=None)
        run("alias b=a",status=None)
        run("a",["argument invalide"],None)
        run(b"xxpwd\x1b[H\x1b[3~\x1b[3~\x1b[F",[b"\r\n/\r\n"])
        run(b"\x1b[A",[b"\r\n/\r\n"])
        run(b"p\x1b[A\x1b[Bwd",[b"\r\n/\r\n"])
        run("@probe stat")
        run("@probe droits")
        run("@probe revocation")
        t.line("@probe faute",has=["processus interrompu"],status="faute 0")
        run("pwd",[b"\r\n/\r\n"])
        # Le destructeur du banc tue QEMU avant la fermeture du volume.
    before=host("stat","/documents/renomme").split()
    assert before[0]==alpha[0] and before[3:5]==alpha[3:5]
    assert start<=int(before[5].split(b".")[0])<=int(time.time())+2
    assert host("lire","/copie")==large and host("lire","/double/exemple.txt")==b"NovaOS\n"
    stamp=host("stat","/vide").split()
    assert all(start<=int(v.split(b".")[0])<=int(time.time())+2 for v in stamp[3:])
    # Les champs non sélectionnés conservent même leur fraction historique.
    for name, field, untouched, old in [("chrono-a", 6, 4, b"789.000000123"),
                                         ("chrono-m", 4, 6, b"123.000000456")]:
        row = host("stat", "/" + name).split()
        assert row[untouched] == old
        assert start <= int(row[field].split(b".")[0]) <= int(time.time()) + 2
        assert host("lire", "/" + name) == b"horloge"
    hidden=host("stat","/.cache").split()
    assert hidden[3:5]==[b"?",b"?"]
    serie=t.archive.read_text(errors="replace")
    launched=re.findall(r"CLI spawn image=(\d+) pid=(\d+)",serie)
    assert {int(image) for image,_ in launched}==set(range(13))
    level="Ring3" if arch=="x86_64" else "EL0"
    pids={pid for _,pid in launched}
    assert pids<=set(re.findall(rf"PROC pid=(\d+) niveau={level} pc=",serie))
    assert pids==set(re.findall(r"CLI collecte pid=(\d+)",serie))
    private=[tuple(map(int,m)) for m in re.findall(r"CLI mem pid=(\d+) privees=(\d+) pile=(\d+)",serie)]
    bypid={}
    for pid, data, stack in private:
        bypid[pid] = max(bypid.get(pid, 0), data + stack)
    memory["pages_privees_phases"]=bypid
    memory["pic_resident_observe_pages"]=max(map(int,re.findall(r" resident=(\d+)",serie)))
    with Terminal(arch,sector,image,"p9-remontage",test=True) as t:
        t.until(b"nova> ")
        t.line(b"\x1b[A",absent=[b"CLI spawn image=", b"commande inconnue"])
        t.line("l",has=["commande inconnue"])
        t.line(b"\x1b[A",has=["commande inconnue"])
        t.line("cat double/exemple.txt",has=["NovaOS"],status="sortie 0")
        t.line("cp copie seconde", status="sortie 0")
        t.line("wc -c seconde", has=[f"{len(large)} seconde"], status="sortie 0")
        t.line("tail -n1 seconde", has=["dernière"], status="sortie 0")
        t.line("rm seconde", status="sortie 0")
        for _ in range(4):
            t.line("pwd", has=[b"\r\n/\r\n"], status="sortie 0")
        t.exit()
    assert host("lire","/copie")==large and host("stat","/documents/renomme").split()==before
    reboot=t.archive.read_text()
    assert re.search(r"RAM phase=bloc_collecte .*ipc=0 utilisateur=0 processus=0",reboot)
    idle = re.findall(r"CLI repos pages=(\d+) libres=(\d+) buddy=(\d+)", reboot)
    assert len(idle) >= 4 and len(set(idle[-4:])) == 1
    frame_report = re.search(r"BLOC memoire cadres_init=(\d+) cadres_fin=(\d+) allocations=(\d+) pic_cadres=(\d+) buddy_init=(\d+) buddy_fin=(\d+)", reboot)
    initial, final, allocations, peak, buddy_start, buddy_end = map(int, frame_report.groups())
    assert buddy_start == buddy_end
    initial_ram = int(re.search(r"RAM phase=bloc_repos .* resident=(\d+)", reboot)[1])
    memory.update(pic_resident_remontage_pages=initial_ram + peak, allocations_cadres_remontage=allocations,
                  cadres_retenus_final=initial-final, buddy_final_octets=buddy_end)
    hz=int(re.search(r"BLOC memoire .* hz=(\d+)",reboot)[1])
    times["montage_apres_coupure_ms"]=1000*int(re.search(r"ECHANTILLON ech montage-cli cycles=(\d+)",reboot)[1])/hz
    times["stat_moyenne_ms"] = 1000 * int(re.search(r"ECHANTILLON ech stat16 cycles=(\d+)", serie)[1]) / (16 * hz)
    stamp_copy=host("stat","/copie").split()
    assert all(start<=int(v.split(b".")[0])<=int(time.time())+2 for v in stamp_copy[3:])
    return dict(architecture=arch,secteur=sector,octets_copie=len(large),ms=times,memoire=memory)

if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--sector",choices=(512,4096),type=int)
    args=parser.parse_args()
    arch=os.environ["NOVA_ARCH"]
    with tempfile.TemporaryDirectory(prefix="nova-cli-p9-") as folder:
        tmp=Path(folder)
        oracle=programme_oracle(arch,tmp)
        for sec in ([args.sector] if args.sector else [512,4096]):
            work=tmp/str(sec)
            work.mkdir()
            stats=check(arch,sec,work,oracle)
            (ROOT/"build"/f"cli-p9-{arch}-{sec}.json").write_text(json.dumps(stats,indent=2,ensure_ascii=False)+"\n")
            print(f"OK — {arch}/{sec} : commandes, dates, contrôle Darwin et remontage",flush=True)
