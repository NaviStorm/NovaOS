"""Chronométrer des fichiers synthétiques distincts sur Darwin ARM64."""
import json
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import tempfile
import time
from laboratoire import TMP, SEED, digest, parser, tools

p=parser(__doc__)
p.add_argument("--large",action="store_true",help="ajouter 100 Mio et 1 Gio au palier de 1 Mio")
p.add_argument("--mio",type=int,choices=[1,100,1024],help="mesurer uniquement ce palier explicite")
p.add_argument("--repetitions", type=int, default=3, help="passages mesurés par outil après échauffement")
p.add_argument("--candidat", type=Path, help="mesurer aussi ce Mach-O face au même GNU")
p.add_argument("--serie", choices=["A","B"], default="A", help="ordre croisé des trois exécutables")
a=p.parse_args()
if a.repetitions < 3:
    p.error("--repetitions exige au moins trois passages")
bins=tools(a)
if a.candidat:
    if set(bins) != {"Nova", "GNU"}:
        p.error("--candidat exige aussi le Mach-O de référence Nova et GNU")
    bins["Candidat"] = a.candidat.resolve()

def ordre_tour(turn, order):
    if not a.candidat:
        return order if turn % 2 == 0 else order[::-1]
    # Deux carrés latins : chaque outil occupe chaque position deux fois en six tours.
    base = ["Nova", "GNU", "Candidat"]
    index = (turn - 1) % 6
    if index >= 3:
        base.reverse()
    offset = index % 3
    seq = base[offset:] + base[:offset]
    return seq if a.serie == "A" else seq[::-1]

sizes=[a.mio] if a.mio else ([1,100,1024] if a.large else [1])
M=1<<20
env={**os.environ,"LANG":"C","LC_ALL":"C"}
env.pop("POSIXLY_CORRECT",None)
report=dict(graine=SEED,systeme=platform.platform(),ram=int(subprocess.check_output(["sysctl","-n","hw.memsize"])),
    echauffements=1, repetitions=a.repetitions, architecture=platform.machine(),
    cache="lecture après échauffement, cache Darwin non purgé",libre=shutil.disk_usage(TMP).free,
    outils={name:dict(path=str(path),sha256=digest(path),version=subprocess.check_output([path,"--version"]).decode().splitlines()[0])
            for name,path in bins.items()},mesures=[])
if report["libre"]<2*max(sizes)*M+128*M:raise SystemExit("espace insuffisant")
if a.candidat:
    report.update(serie=a.serie, debut_unix_ns=time.time_ns(), charge_initiale=os.getloadavg(),
                  modele=subprocess.check_output(["sysctl","-n","hw.model"]).decode().strip())
col_candidat = f" | {'Candidat':>28}" if a.candidat else ""
print(f"{'Taille ou lignes':>16} | {'Traitement':<18} | {'Nova cmp':>28} | {'GNU cmp':>28}{col_candidat}", flush=True)
with tempfile.TemporaryDirectory(dir=TMP,prefix="cmp-mesures-") as tmp:
    work=Path(tmp)
    left,right=work/"gauche",work/"droite"
    chunk=random.Random(SEED).randbytes(M)
    for size in sizes:
        n=size*M
        # Deux écritures indépendantes. Ni clone APFS ni lien physique.
        for path in [left,right]:
            with path.open("wb") as f:
                for _ in range(size):f.write(chunk)
        assert left.stat().st_ino!=right.stat().st_ino
        def modify(positions,flip):
            with right.open("r+b") as f:
                for i in positions:
                    f.seek(i);f.write(bytes([chunk[i%M]^flip]))
        for label,positions,flags,full in [
            ("égaux",[],[],n),("début",[0],[],0),("milieu",[n//2],[],0),("fin",[n-1],[],n),
            ("EOF",[],[],n-1),("limite",[],["-n",str(n//4)],n//4),
            ("deux sauts",[],["-i","17:17"],n-17),("sauts distincts",[],["-i","17:23"],0),
            ("liste clairsemée",[0,n//2,n-1],["-l"],n),("même objet",[],[],0)]:
            modify(positions,1)
            if label=="EOF":
                with right.open("r+b") as f:f.truncate(n-1)
            argv=[*flags,str(left),str(left if label=="même objet" else right)]
            pos=positions[0]+1 if positions else (n if label=="EOF" else (1 if label=="sauts distincts" else None))
            expected=int(pos is not None)
            row=dict(taille_octets=n,traitement=label,premier_ecart=pos,parcours_pour_debit=full,
                     arguments=flags,differences=[i+1 for i in positions],mesures={},ordre=[])
            for name in bins:row["mesures"][name]=[]
            order=list(bins)
            # Les trois tailles sont des multiples du bloc du GNU 3.12 livré.
            row["statuts_attendus"] = {
                name: 0 if name == "GNU" and label == "liste clairsemée" else expected
                for name in bins
            }
            sorties = None
            for turn in range(1 + report["repetitions"]):
                for name in ordre_tour(turn, order):
                    # Même fichier ordinaire pour les deux outils. /dev/null modifie le chemin GNU.
                    out=work/"stdout";err=work/"stderr"
                    with out.open("wb") as stdout,err.open("wb") as stderr:
                        actions=[(os.POSIX_SPAWN_DUP2,stdout.fileno(),1),(os.POSIX_SPAWN_DUP2,stderr.fileno(),2)]
                        start=time.perf_counter_ns()
                        pid=os.posix_spawn(str(bins[name]),["cmp",*argv],env,file_actions=actions)
                        _,status,usage=os.wait4(pid,0)
                        elapsed_ns=time.perf_counter_ns()-start
                        elapsed=elapsed_ns*1e-9
                    rc=os.waitstatus_to_exitcode(status)
                    data=out.read_bytes();diag=err.read_bytes()
                    if sorties is None:
                        sorties = (data, diag)
                    if (data, diag) != sorties:
                        raise AssertionError((label, name, "stdout ou stderr divergent", data, diag))
                    if label=="liste clairsemée":
                        assert [int(line.split()[0]) for line in data.splitlines()]==[i+1 for i in positions]
                    elif label=="EOF":assert f"after byte {n-1},".encode() in diag
                    elif expected:assert f"char {pos}, line ".encode() in data
                    else:assert not data and not diag
                    if rc != row["statuts_attendus"][name]:
                        raise AssertionError((label, name, rc, row["statuts_attendus"][name]))
                    if turn:
                        row["ordre"].append(name)
                        row["mesures"][name].append(dict(nanosecondes=elapsed_ns,secondes=elapsed,rss_octets=usage.ru_maxrss,statut=rc))
            if a.candidat:
                row["stdout_hex"], row["stderr_hex"] = [stream.hex() for stream in sorties]
                row["echauffement_ordre"] = ordre_tour(0, order)
                report.update(fin_unix_ns=time.time_ns(), charge_finale=os.getloadavg())
            report["mesures"].append(row)
            row["synthese"] = {}
            def cellule(name):
                if name not in row["mesures"]:
                    return "—"
                samples = row["mesures"][name]
                median = statistics.median(sample["secondes"] for sample in samples)
                rate = full/M/median if full else None
                row["synthese"][name] = dict(mediane_secondes=median, debit_mio_s=rate,
                                             pic_rss_octets=max(sample["rss_octets"] for sample in samples))
                value = f"{median*1000:.3f} ms"
                if rate is not None:
                    value += f" · {rate:.1f} Mio/s"
                return value
            taille = "1 Gio" if size == 1024 else f"{size} Mio"
            col_candidat = f" | {cellule('Candidat'):>28}" if a.candidat else ""
            print(f"{taille:>16} | {label:<18} | {cellule('Nova'):>28} | {cellule('GNU'):>28}{col_candidat}", flush=True)
            if a.json:a.json.write_text(json.dumps(report,ensure_ascii=False,indent=1)+"\n")
            modify(positions,0)
            if label=="EOF":
                with right.open("ab") as f:f.write(chunk[-1:])
