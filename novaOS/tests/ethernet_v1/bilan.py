"""Recalcule le comparatif depuis les séries brutes, sans démarrer de VM."""
import argparse
from collections import defaultdict
import gzip
import itertools
import json
import math
from pathlib import Path
from statistics import median

MODES=("data","boite","fenetre")
KEYS=set(itertools.product((1,2,4,8),(60,1514),MODES))

def quantile(samples,percent):
    return sorted(samples)[math.ceil(len(samples)*percent/100)-1]

def counters(batch,length,mode):
    size=batch*length
    if mode=="data":
        fragments=batch*math.ceil(length/96)
        return 4+2*fragments,4*fragments,4*size
    if mode=="boite":
        groups=math.ceil(batch/(4096//length))
        return 4+2*groups,2*groups,2*size
    return 4,2*batch,2*size

def series_host(data):
    table=defaultdict(list)
    for row in data:
        b,l,mode=row["b"],row["len"],row["mode"]
        lat=row["ns"]
        n=len(lat)
        assert n==4096 and len(row["retrait_ns"])==129 and sum(lat)>0
        calls,copies,size=counters(b,l,mode)
        assert [row[k] for k in ("calls","copies","bytes")]==[n*x for x in (calls,copies,size)]
        table[b,l,mode].append(dict(
            serie=row["series"],calls=calls,copies=copies,bytes=size,
            MB_s=b*l*n*1000/sum(lat),p50=quantile(lat,50),p95=quantile(lat,95),p99=quantile(lat,99),
            cpu_ns_frame=row["cpu_ns"]/(b*n),cycles_frame=row["cycles"]/(b*n) if row["cycles"] else None,
            switches=row["switches"],wakes=row["wakes_irq"],rss=row["rss"],peak=row["peak"],baseline=row["rss_base"],
            retrait_ns=quantile(row["retrait_ns"],50),model_bytes=row["model_bytes"]))
    assert set(table)==KEYS
    rows=[]
    for (b,l,mode),runs in sorted(table.items()):
        assert sorted(r["serie"] for r in runs)==[1,2,3]
        row={key:median(r[key] for r in runs) if all(r[key] is not None for r in runs) else None
             for key in runs[0] if key!="serie"}
        rows.append(dict(b=b,length=l,mode=mode,**row))
    return rows

def fields(line):
    result={}
    for pair in line.split()[2:]:
        k,v=pair.split("=")
        result[k]=int(v) if v.isdigit() else v
    return result

def series_guest(text):
    lots=defaultdict(list)
    resources={}
    assert text.count("ETH2 campagne OK")==1 and "Panique du noyau" not in text
    assert text.count("ETH2 isolation paires=2 croisement_refuse=1 revocation_locale=1 voisin_intact=1 pages_neuves_zero=2 solde=0")==1
    for line in text.splitlines():
        if line.startswith("ETH2 lot "):
            x=fields(line)
            lots[x["b"],x["len"],x["mode"]].append(x)
        if line.startswith("ETH2 ressources "):
            x=fields(line)
            key=x["b"],x["len"],x["mode"]
            assert key not in resources
            resources[key]=x
    assert set(lots)==set(resources)==KEYS
    rows=[]
    for (b,l,mode),series in sorted(lots.items()):
        assert [x["tour"] for x in series]==list(range(17))
        calls,copies,size=counters(b,l,mode)
        assert all((r["calls"],r["copies"],r["bytes"])==(calls,copies,size) for r in series)
        r=resources[b,l,mode]
        assert r["base"]=="0/0/0/0" and r["racines"]=="2/0" and r["solde"]==0
        assert r["ouvert"]==("2/2/5/1" if mode=="boite" else "2/2/4/0")
        assert r["libres_fin"]==r["libres_base"]
        observed=series[1:]
        lat=[x["ticks"]*1e9/x["hz"] for x in observed]
        rows.append(dict(b=b,length=l,mode=mode,calls=calls,copies=copies,bytes=size,
            MB_s=b*l*len(lat)*1000/sum(lat),p50=quantile(lat,50),p95=quantile(lat,95),p99=quantile(lat,99),
            switches=sum(x["switches"] for x in observed)/len(lat),wakes=sum(x["wakes"] for x in observed)/len(lat),
            open_kib=(r["libres_base"]-r["libres_ouvert"])*4,
            peak_kib=(r["libres_base"]-r["libres_min_echant"])*4,
            retrait_ns=r["retrait_ticks"]*1e9/r["hz"],root_src=2,root_dst=0))
    return rows

def decisions(rows):
    cells={(r["b"],r["length"],r["mode"]):r for r in rows}
    gains={}
    for b in (4,8):
        ref=max(cells[b,1514,m]["MB_s"] for m in ("boite","data"))
        gains[b]=100*(cells[b,1514,"fenetre"]["MB_s"]/ref-1)
    tails={}
    for l in (60,1514):
        ref=min(cells[1,l,m]["p99"] for m in ("boite","data"))
        tails[l]=100*(cells[1,l,"fenetre"]["p99"]/ref-1)
    return dict(gain_pct=gains,regression_p99_pct=tails,seuils=all(v>=10 for v in gains.values()) and all(v<=10 for v in tails.values()))

def fmt(value,precision=1):
    if value is None:return "n.d."
    return f"{value:.{precision}f}".replace(".",",")

def markdown(all_rows):
    lines=["# Mesures Ethernet v1 — phase 2","",
           "Un lot correspond à une admission sur un seul saut. Les débits sont en Mo/s décimaux de trames fictives. DATA inclut fragmentation et réassemblage. Les copies comptent les transferts logiques de charge utile. Les copies internes de structures par le noyau, l’oracle et les transferts de lignes de cache restent hors de ce compteur.",""]
    for platform,rows in all_rows.items():
        host=platform.startswith("Darwin")
        lines+=["## "+platform,""]
        if host:
            lines+=["Médiane de trois séries de 4 096 lots. Latences et retrait CPU en ns. Cycles matériels et temps CPU en ns par trame sur la série, préparation et boucle incluses. Commutations et réveils d’interruption Darwin par série. RSS et maximum du processus en Kio. Les appels IPC sont modélisés. Le débit porte uniquement sur la section chronométrée de chaque lot.","",
               "| Lot | Octets/trame | Transport | Appels/lot | Copies/lot | Octets copiés/lot | Mo/s | p50 / p95 / p99 (ns) | Cycles / ns CPU par trame | Commutations / réveils | RSS / pic (Kio) | Retrait (ns) |"]
        else:
            lines+=["Une série de 16 lots après échauffement par cellule. Latences et retrait en µs. Appels IPC effectivement réussis, commutations et réveils de l’ordonnanceur par lot, avec orchestration. RAM ouverte et pic échantillonné en Kio additionnels. Le p99 est le maximum des 16 observations. Les compteurs de durée de l’invité ne mesurent pas les cycles CPU.","",
               "| Lot | Octets/trame | Transport | Appels/lot | Copies/lot | Octets copiés/lot | Mo/s | p50 / p95 / p99 (µs) | Commutations / réveils | Ouverture / pic (Kio) | Retrait (µs) |"]
        cols=12 if host else 11
        lines.append("|"+"---|"*cols)
        for x in rows:
            div=1 if host else 1000
            vals=[str(x["b"]),str(x["length"]),x["mode"],fmt(x["calls"],0),fmt(x["copies"],0),fmt(x["bytes"],0),
                  fmt(x["MB_s"],2)," / ".join(fmt(x[k]/div,0 if host else 1) for k in ("p50","p95","p99"))]
            if host: vals.append(fmt(x["cycles_frame"])+" / "+fmt(x["cpu_ns_frame"]))
            vals.append(fmt(x["switches"],0 if host else 1)+" / "+fmt(x["wakes"],0 if host else 1))
            vals.append((fmt(x["rss"]/1024,0)+" / "+fmt(x["peak"]/1024,0)) if host else (str(x["open_kib"])+" / "+str(x["peak_kib"])))
            vals.append(fmt(x["retrait_ns"]/div,0 if host else 1))
            lines.append("| "+" | ".join(vals)+" |")
        lines+=[""]
    lines+=["## Application des seuils","",
            "Gain de débit des fenêtres face au meilleur concurrent à 1 514 octets. Variation du p99 des fenêtres face au meilleur concurrent avec un lot unitaire. Une variation négative indique une latence inférieure.","",
            "| Profil | Gain B=4 | Gain B=8 | Variation p99 60 | Variation p99 1 514 | Seuils de performance |","|---|---:|---:|---:|---:|---|"]
    for name,rows in all_rows.items():
        d=decisions(rows)
        values=[d["gain_pct"][4],d["gain_pct"][8],d["regression_p99_pct"][60],d["regression_p99_pct"][1514]]
        lines.append("| "+name+" | "+" | ".join(fmt(v)+" %" for v in values)+" | "+("atteints" if d["seuils"] else "non atteints")+" |")
    return "\n".join(lines)+"\n"

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("preuves",type=Path)
    parser.add_argument("sortie",type=Path)
    parser.add_argument("--json",type=Path)
    parser.add_argument("--repetitions", action="store_true",
                        help="Vérifier aussi les deux démarrages supplémentaires de chaque invité")
    opts=parser.parse_args()
    profiles={}
    for isa in ("aarch64","x86_64"):
        raw=gzip.decompress((opts.preuves/f"darwin-{isa}.json.gz").read_bytes())
        profiles["Darwin "+("ARM64 natif" if isa=="aarch64" else "x86_64 sous Rosetta")]=series_host(json.loads(raw))
    for isa in ("aarch64","x86_64"):
        profiles["NovaOS QEMU "+isa]=series_guest(gzip.decompress((opts.preuves/f"qemu-{isa}.log.gz").read_bytes()).decode())
    report = markdown(profiles)
    evidence = {name: {"cellules": rows, "decision": decisions(rows)}
                for name, rows in profiles.items()}
    if opts.repetitions:
        table = ["", "## Répétabilité en invité", "",
                 "Trois démarrages indépendants avec le même ELF par ISA. Chaque p99 unitaire est le maximum de 16 lots après une chauffe. Les répétitions 2 et 3 sont diagnostiques. La campagne 1 reste la référence des tableaux précédents.", "",
                 "| ISA | Démarrage | Gain débit B=4 / B=8 à 1 514 | p99 B=1 à 60, fenêtre / meilleur concurrent (µs) | Variation p99 à 60 / 1 514 |",
                 "|---|---:|---:|---:|---:|"]
        for isa in ("aarch64", "x86_64"):
            name = "NovaOS QEMU " + isa
            by_boot = []
            for boot in (1, 2, 3):
                if boot == 1:
                    rows = profiles[name]
                else:
                    path = opts.preuves / f"qemu-{isa}-{boot}.log.gz"
                    rows = series_guest(gzip.decompress(path.read_bytes()).decode())
                decision = decisions(rows)
                unit = {row["mode"]: row["p99"] / 1000 for row in rows
                        if row["b"] == 1 and row["length"] == 60}
                best = min(unit["boite"], unit["data"])
                gain = decision["gain_pct"]
                tail = decision["regression_p99_pct"]
                table.append(f"| {isa} | {boot} | {fmt(gain[4])} % / {fmt(gain[8])} % "
                             f"| {fmt(unit['fenetre'])} / {fmt(best)} "
                             f"| {fmt(tail[60])} % / {fmt(tail[1514])} % |")
                by_boot.append(dict(demarrage=boot, decision=decision, p99_60_us=unit))
            evidence[name]["repetabilite"] = by_boot
        report += "\n".join(table) + "\n"
    opts.sortie.write_text(report)
    if opts.json:
        opts.json.write_text(json.dumps(evidence, indent=2) + "\n")
if __name__=="__main__":main()
