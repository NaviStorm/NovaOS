"""Consolider les preuves 3C à partir des démarrages et des captures non modifiés."""
import argparse
import gzip
import json
from pathlib import Path
import re
from statistics import median
import struct
import tarfile

import cycle_vie
from performance_bilan import ARCH, COUTS, NIC, PAIR, TYPE, digest, exiger, lire, machine, run
from port_qemu import vtd_trafic

CAS_COMMUNS = ("cycles", "cycles-court", "rattrapage", "court", "port", "deces",
               "deces-recu-tx-absent", "incertain", "revocation", "segment-interrompu", "recu-collecte", "preuve-absente")
SPECIFIQUES = {"arm": ("recu-arm-differe", "recu-arm-absent"),
               "q35": ("recu-vtd-differe", "recu-vtd-absent")}
REPLIS = ("choix-data", "plein")
TEMOIN = "ff864f5bec14315e7374375a92ca47912a0abdc5"
TEMOIN_CLOTURE = "1a7de0e80b91e8b94525ef1e337d8daed97e0406"
AUDIT = "56db7d5c0d04490cd284b91567cbbd4736534266"


def trames(p, cas, attendues):
    """Reconstitue les octets des deux sens depuis la séquence locale de chaque cycle."""
    comptes = [0, 0]
    octets = [0, 0]
    with gzip.open(p/"pair.pcap.gz", "rb") as f:
        exiger(f.read(24) == struct.pack("<IHHIIII", 0xa1b2c3d4, 2, 4, 0, 0, 65535, 1), p)
        while header := f.read(16):
            _, _, n, original = struct.unpack("<4I", header)
            exiger(n == original and n in (60,1514), p)
            data = f.read(n)
            sens = (PAIR+NIC, NIC+PAIR).index(data[:12])
            numero = comptes[sens]
            if cas in ("cycles", "recu-arm-differe"):
                numero %= 144
            elif cas in ("cycles-court", "recu-vtd-differe"):
                numero = 0
            elif cas == "rattrapage":
                numero %= 16
            exiger(n == (1514 if numero%2 else 60) and data[12:14] == TYPE, (p,numero))
            charge = bytes((29*numero+17*i)&255 for i in range(n-15))
            if sens:
                charge = bytes(v ^ 165 for v in charge)
            motif = bytes([numero]) + charge if numero<16 else numero.to_bytes(4,"big")+charge[3:]
            exiger(data[14:] == motif, (p,numero,sens))
            comptes[sens] += 1
            octets[sens] += n
    exiger(comptes == attendues, (p,comptes,attendues))
    return dict(trames=comptes,octets=octets)


def trafic_disque_pendant_retrait(p, cas, cycles):
    # Le couple UNMAP des pages 21 puis 0 identifie la commande mise en attente.
    # Vérifier les accès réels du SID disque avant le premier retrait suivant.
    precedent = None
    traductions = None
    intervalles = []
    with (p/"qemu.trace").open() as trace:
        for ligne in trace:
            if ligne.startswith("virtio_iommu_unmap_done "):
                op = re.search(r"domain=(\d+) virt_start=(0x\w+)",ligne)
                domaine, iova = int(op[1]), int(op[2],16)
                if traductions is not None:
                    attendu = (1,0x8006000) if cas=="recu-arm-absent" else (3,0x10014000)
                    exiger((domaine,iova)==attendu and min(traductions)>0,(p,domaine,iova,traductions))
                    intervalles.append(dict(lectures=traductions[0],ecritures=traductions[1]))
                    traductions=None
                if precedent==(3,0x10015000) and (domaine,iova)==(3,0x10000000):
                    traductions=[0,0]
                precedent=domaine,iova
            elif traductions is not None and ligne.startswith("virtio_iommu_translate ") and " rid=48 " in ligne:
                droit=int(ligne.split("flag=")[1])
                exiger(droit in (1,2),p)
                traductions[droit-1]+=1
    exiger(traductions is None and len(intervalles)==cycles,p)
    return intervalles


def verifier_cycle(p, revision, machines):
    c = lire(p/"campagne.json")
    cas, arch = c["cas"], c["arch"]
    serie = (p/"serie.log").read_text()
    exiger(c["succes"] and c["profil_verifie"] and c["commit_sources"]==revision
           and digest(p/"novaos.elf")==c["elf"]==c["construction"]["elf"][arch], p)
    signature = machine(p,c)
    exiger(signature==machines.setdefault(arch,signature),p)
    pairs = c["trames_par_sens"]
    fil = trames(p,cas,[pairs,pairs+c["rx_injectees_perf"]])
    if cas in REPLIS:
        exiger(c["transport_client"]==1 and "racines=[0, 4, 1]" in serie,p)
        contrat = re.search(r"boites_globales=(\d+) endpoints_globaux=(\d+) ipc_pages=(\d+)",serie)
        attendu = ("8","13","23") if cas=="plein" else ("2","5","9")
        exiger(contrat and contrat.groups()==attendu,(p,contrat))
        return dict(fil=fil,ressources_montage=list(map(int,attendu)),transport="DATA")
    preuve = cycle_vie.verifier(p,c,serie)
    exiger(serie.count("generation_verrouillee=true")==len(preuve["generations"]),p)
    exiger(json.loads(json.dumps(preuve))==c["cycle_vie"],p)
    exiger(c["transport_client"]==3 and len({n for n,_ in preuve["cadres_cpu"]})==1,p)
    if arch=="q35":
        exiger(vtd_trafic(p, len(preuve["generations"]))==c["dma"],p)
    bilans = preuve["ram"]
    for debut,fin in zip(bilans["eth3_initial"],bilans["eth3_retrait"],strict=True):
        for champ in ("libres","tables","kpile","upile","ipc","utilisateur","processus","dma"):
            exiger(debut[champ]==fin[champ],(p,champ))
    intervalles = trafic_disque_pendant_retrait(p,cas,len(preuve["generations"])) if cas in ("recu-arm-differe","recu-arm-absent") else []
    return dict(fil=fil,generations=preuve["generations"],cadres_cpu=preuve["cadres_cpu"],
                dma_disque_pendant_retrait=intervalles,
                memoire_finale=preuve["memoire_finale"],ram=bilans,arrets=preuve["arrets"],
                iommu=preuve.get("iommu",c.get("preuve_qi")),bilans=preuve["bilans"])


def verifier_duo(p, revision):
    c = lire(p/"campagne.json")
    source = lire(p/"sources.json")
    cas = c["cas"]
    exiger(c["succes"] and c["elf"]==source["elf"][cas] and source["port"]==revision and source["audit"]==AUDIT
           and len(source["modules"])==15 and digest(p/"novaos.elf")==source["elf"][cas],p)
    exiger(digest(p/"namespace.raw")==c["p9_final"],p)
    s = c["stockage"]
    exiger(s["operations"]==32 and s["io_apres_retrait_nic"]>0 and s["recouvrement"]
           and s["ordre"]["confirmation_apres_f2"] and s["p9"]["etat"]=="nouveau"
           and len(s["p9"]["ancres"])==2 and max(v for _,v in s["racines"])<=4,p)
    serie = (p/"serie.log").read_text()
    gardes = int(re.search(r"RAM phase=nvme_apres_retrait [^\n]*?\bdma=(\d+)",serie)[1])
    exiger(gardes==(34 if cas=="deces" else 8),p)
    n = 4 if cas=="deces" else 144
    return dict(fil=trames(p,cas,[n,n]),stockage=s,p9_final=c["p9_final"],dma_final=gardes,
                tables_finales=c.get("tables_finales"),sources=source)


def mesure_echo(p, source, machines):
    boot=run(p,"echo1514",machines)
    exiger(boot["sources"]==source,p)
    c=lire(p/"campagne.json")
    if source not in (TEMOIN, TEMOIN_CLOTURE):
        exiger(json.loads(json.dumps(cycle_vie.verifier(p,c,(p/"serie.log").read_text())))==c["cycle_vie"],p)
    cell=boot["cellules"]["8"]
    temps=[t*1000/boot["hz"] for t in cell["quantiles_ticks"]]
    cout=cell["cout"]
    operations=("attentes_bloquees","changements_contexte","reveils","query","publications")
    par_lot={k:cout[COUTS.index(k)]/cell["n"] for k in operations}
    return dict(n=cell["n"],tx_mbps=cell["tx_mbps"],p99_ms=cell["p99_ms"],
                ipc_par_trame=cell["ipc_par_trame"],quantiles_ms=temps,cout_par_lot=par_lot,
                pair_mbps=cell["pair_mbps"],acquisition=c["acquisition"],elf=c["elf"])


def paquet(sortie, base, entrees):
    index = {}
    with (sortie/"preuves.tar.gz").open("wb") as brut, gzip.GzipFile(filename="",fileobj=brut,mode="wb",mtime=0) as gz, tarfile.open(fileobj=gz,mode="w|") as tar:
        for dossier in sorted(entrees):
            for p in sorted(dossier.iterdir()):
                if not p.is_file():
                    continue
                nom = str(p.relative_to(base))
                index[nom] = dict(taille=p.stat().st_size,sha256=digest(p))
                # Les gros volumes, ELF et traces de performance restent indexés dans l'archive brute.
                if p.suffix in (".json",".log") or (p.name in ("qemu.trace","pair.pcap.gz") and
                                                     p.stat().st_size<=2_000_000 and not nom.startswith("echo-")):
                    info=tarfile.TarInfo(nom)
                    info.size=p.stat().st_size
                    with p.open("rb") as f:
                        tar.addfile(info,f)
    return index


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archives",type=Path)
    parser.add_argument("revision")
    parser.add_argument("sortie",type=Path)
    parser.add_argument("--contre-q35", action="store_true", help="Ajouter les deux paires diagnostiques Q35 séparément")
    parser.add_argument("--cloture-3b", action="store_true", help="Relire le second témoin, construit sur le code livré de phase 3B")
    args=parser.parse_args()
    base,rev,out=args.archives,args.revision,args.sortie
    suffixe=rev[:7]
    machines,matrice,duo,perf,series,entrees={},{},{},{},{},set()
    for arch in ARCH:
        matrice[arch]={}
        for cas in (*CAS_COMMUNS,*SPECIFIQUES[arch],*REPLIS):
            p=base/("validation-"+suffixe)/cas/arch/"1"
            matrice[arch][cas]=verifier_cycle(p,rev,machines)
            entrees.update((p,base/("images-"+suffixe)/cas))
    for cas in ("port","deces"):
        p=base/("duo-"+suffixe)/cas/"q35"
        duo[cas]=verifier_duo(p,rev)
        entrees.add(p)
    entrees.add(base/("images-duo-"+suffixe))
    entrees.add(base/("images-"+suffixe)/"echo1514")
    for arch in ARCH:
        perf[arch]={}
        for nom,source in (("3b",TEMOIN),("3c",rev)):
            mesures=[]
            for i in range(1,4):
                p=base/("echo-"+nom+"-"+suffixe)/"echo1514"/arch/str(i)
                mesures.append(mesure_echo(p,source,machines))
                entrees.add(p)
            series[arch+"-"+nom]=mesures
            perf[arch][nom]={k:median(c[k] for c in mesures) for k in ("tx_mbps","p99_ms","ipc_par_trame")}
    contre={}
    if args.contre_q35:
        for nom,source in (("3b",TEMOIN),("3c",rev)):
            contre[nom]=[]
            for i in (1,2):
                p=base/("contre-"+nom+"-"+suffixe)/"echo1514"/"q35"/str(i)
                contre[nom].append(mesure_echo(p,source,machines))
                entrees.add(p)
    cloture={}
    if args.cloture_3b:
        for arch in ARCH:
            cloture[arch]=[]
            for i in range(1,4):
                p=base/("cloture-3b-"+suffixe)/"echo1514"/arch/str(i)
                cloture[arch].append(mesure_echo(p,TEMOIN_CLOTURE,machines))
                entrees.add(p)
    natif=base/("contrat-"+suffixe)
    for arch in ("arm64","x86_64"):
        c=lire(natif/(arch+".json"))
        exiger(c["revision"]==rev and digest(natif/("contrat-"+arch))==c["sha256"]
               and "12 passed; 0 failed" in (natif/(arch+".log")).read_text(), arch)
    exiger("Ran 9 tests" in (natif/"abi.log").read_text() and "OK" in (natif/"abi.log").read_text(),natif)
    entrees.add(natif)
    out.mkdir(parents=True,exist_ok=True)
    index=paquet(out,base,entrees)
    source_p9 = base.parent/"archives-virtio-net/cloture-20261126/confirmation-arm-endurance/volume-relu.p9"
    principaux=sum(map(len,matrice.values()))+len(duo)+sum(map(len,series.values()))
    resultat=dict(revision=rev,audit=AUDIT,temoin=TEMOIN,machines=machines,cycle_vie=matrice,
                  reference_p9_arm=dict(sha256=digest(source_p9),octets=source_p9.stat().st_size,offset=61<<20),
                  q35_duo=duo,performances=perf,demarrages_echo=series,
                  contre_epreuve_q35=contre,demarrages_diagnostiques=sum(map(len,contre.values())),
                  temoin_cloture_3b=cloture,demarrages_principaux=principaux,
                  demarrages_qemu=principaux+sum(map(len,contre.values()))+sum(map(len,cloture.values())))
    for nom,contenu in (("bilan.json",resultat),("inventaire.json",index)):
        (out/nom).write_text(json.dumps(contenu,ensure_ascii=False,sort_keys=True,indent=2)+"\n")
    (out/"SHA256SUMS").write_text("".join(f"{digest(out/nom)}  {nom}\n" for nom in ("bilan.json","inventaire.json","preuves.tar.gz")))


if __name__=="__main__":
    main()
