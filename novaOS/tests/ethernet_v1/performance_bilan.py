"""Vérifie les captures et consolide les séries de performance Ethernet 3B."""
import argparse
import ast
from functools import lru_cache
import gzip
import hashlib
import json
from pathlib import Path
import re
from statistics import median
import struct
import io
import tarfile

MODES = [d+str(s) for d in ("echo","tx","rx","duplex") for s in (60,1514)]
ARCH = ("arm","q35")
LOTS = (1,4,8)
TOTAL = 13*1024
NIC = bytes.fromhex("525400123456")
PAIR = bytes.fromhex("024e56000001")
TYPE = bytes.fromhex("88b5")
COUTS = ["ipc_client_service","ipc_service_pilote","ipc_racine_materielle","copies_boite","octets_copies_boite",
         "reserve","attentes_bloquees","changements_contexte","reveils","yield_acteurs","ipc_notify",
         "tx","query","rx","release","ack","preparations","ticks_preparations","collectes_tx","ticks_collectes_tx",
         "lectures_rx","ticks_lectures_rx","rearmements","ticks_rearmements","publications","ticks_publications",
         "autres_operations","ticks_autres_operations","trames_publiees","octets_rx_avec_entete_prive"]
DETAILS = ["ipc_client_service","ipc_service_pilote","ipc_racine_materielle","attente_client","attente_service",
           "attente_pilote","copie_put","copie_take_et_effacement","admission_tx","consultation_recu",
           "demande_rx","restitution_rx","acquittement_ticket"]


def lire(p):
    return json.loads(p.read_text())


def digest(p):
    with p.open("rb") as stream:
        return hashlib.file_digest(stream,"sha256").hexdigest()


def exiger(condition, contexte):
    if not condition:
        raise ValueError(str(contexte))


@lru_cache(maxsize=1024)
def motif(size, low, rx):
    charge = bytes((((29*low+17*i)&255) ^ (0xa5 if rx else 0)) for i in range(size-15))
    return (NIC+PAIR if rx else PAIR+NIC)+TYPE+bytes([low])+charge


def machine(p, camp):
    args=lire(p/"arguments.json")
    sig={k:[args[i+1] for i in range(len(args)-1) if args[i]==k]
         for k in ("-machine","-accel","-cpu","-smp","-m","-device","-global","-trace")}
    sig.update(modele=camp["modele"],version=camp["qemu"])
    if camp["arch"]=="arm":
        sig.update({k:camp[k] for k in ("distribution","firmware","limine_arm","relais")})
    else:
        # Limine scelle le BLAKE2b de chaque noyau. L'EFI et les variables
        # enrôlées ont des empreintes propres, conservées avec chaque démarrage.
        elf=p/"novaos.elf"
        with elf.open("rb") as stream:
            kernel_hash=hashlib.file_digest(stream,"blake2b").hexdigest()
        config=("timeout: 0\nserial: yes\neditor_enabled: no\nhash_mismatch_panic: yes\n"
                "\n/NovaOS\n    protocol: limine\n    path: boot():/novaos.elf#"+kernel_hash+
                "\n    cmdline: net-q35-ethernet\n").encode()
        exiger(hashlib.sha256(config).hexdigest()==camp["entrees"]["limine.conf"],p)
        sig["firmware"]=camp["entrees"]["OVMF_CODE.fd"]
    return sig


def capture(p, mode, taille):
    """Relecture avec un oracle distinct du générateur du pair, borné à deux tailles."""
    source=p/"pair.pcap.gz"
    compte=[0,0]
    limites=[[[None,None] for _ in LOTS] for _ in range(2)]
    horloge=[[],[]]
    fenetre=[hashlib.sha256(),hashlib.sha256()]
    longue=mode in ("mixte100k","historique100k","historique1514")
    with gzip.open(source,"rb") as trace:
        exiger(trace.read(24)==struct.pack("<IHHIIII",0xa1b2c3d4,2,4,0,0,65535,1),source)
        while header:=trace.read(16):
            sec,us,copie,size=struct.unpack("<IIII",header)
            exiger(copie==size and size in (60,1514),source)
            data=trace.read(size)
            rx=data[:6]==NIC
            sens=int(rx)
            seq=compte[sens]
            longueur=taille or (60 if seq%2==0 else 1514)
            base=motif(longueur,seq&255,rx)
            ref=base if seq<16 and not longue else base[:14]+seq.to_bytes(4,"big")+base[18:]
            exiger(data==ref,(p,"rx" if rx else "tx",seq))
            compte[sens]+=1
            stamp=sec*1000000+us
            if mode in MODES:
                i=0 if seq<1024 else 1 if seq<5120 else 2
                win=limites[sens][i]
                if win[0] is None: win[0]=stamp
                win[1]=stamp
            # Même fenêtre physique d'observation des grandes trames, lots publics de 8
            # et publications historiques de 16. Le premier groupe de transition est exclu.
            if mode in ("echo1514","historique1514"):
                if 5136<=seq<13312:
                    fenetre[sens].update(data)
                if 5136<=seq+1<=13312 and (seq+1)%16==0:
                    horloge[sens].append(stamp)
    if mode in MODES:
        tx=0 if mode.startswith("rx") else TOTAL
        rx=16 if mode.startswith("tx") else TOTAL
    else:
        tx=rx=500000 if mode=="historique1514" else 100000
    exiger(compte==[tx,rx],(p,compte))
    stats=dict(trames_tx=tx,trames_rx=rx,sha256_pcap_gz=digest(source))
    if horloge[0]:
        for times in horloge:
            exiger(len(times)==512 and all(y>=x for x,y in zip(times,times[1:])),p)
        t=horloge[0]
        delta=sorted(y-x for x,y in zip(t,t[1:]))
        stats["fenetre_1514"]=dict(trames=8176,octets=8176*1514,duree_tx_us=t[-1]-t[0],
                                    duree_rx_us=horloge[1][-1]-horloge[1][0],p99_inter_tx_16_us=delta[(99*len(delta)+99)//100-1],
                                    empreintes=[f.hexdigest() for f in fenetre])
    return stats,limites


def entier(texte,nom):
    return int(re.search(r"\b"+nom+r"=(\d+)",texte)[1])


def run(p,mode,machines,detail=False):
    c=lire(p/"campagne.json")
    signe=machine(p,c)
    exiger(signe==machines.setdefault(c["arch"],signe),(p,"modèle, firmware ou topologie différents"))
    exiger(c["succes"] and not c["erreur_pair"] and c["elf"]==digest(p/"novaos.elf"),p)
    size=1514 if mode.endswith("1514") else 60 if mode.endswith("60") else None
    serie=(p/"serie.log").read_text()
    boot=dict(arch=c["arch"],mode=mode,demarrage=int(p.name),sources=c["commit_sources"],elf=c["elf"],
              acquisition=c["acquisition"],cpu_qemu=c["cpu_qemu"],preuve_serie=digest(p/"serie.log"))
    if c["arch"]=="q35": boot["entrees_demarrage"]=c["entrees"]
    if "sha256_lanceur" in c: boot["sha256_lanceur"]=c["sha256_lanceur"]
    if "construction" in c:
        boot["construction"]=c["construction"]
    if "provenance_elf" in c:
        boot["provenance_elf"]=c["provenance_elf"]
    if mode not in MODES:
        boot.update(capture(p,mode,size)[0])
        if "fenetre_pair" in c:
            horloge=lire(p/"groupes-pair-ns.json")
            exiger(len(horloge)==6250 and all(x<=y for x,y in zip(horloge,horloge[1:])),p)
            fenetre=horloge[15:-16]
            differences=sorted(y-x for x,y in zip(fenetre,fenetre[1:]))
            preuve=c["fenetre_pair"]
            exiger(preuve["duree_ns"]==fenetre[-1]-fenetre[0] and preuve["trames"]==16*len(differences),p)
            exiger(preuve["p99_intervalles_16_ns"]==differences[(99*len(differences)+99)//100-1],p)
            boot["fenetre_pair"]=preuve
        return boot
    if detail:
        exiger(c["construction"]["diagnostic"],p)
        limites=None
    else:
        stats,limites=capture(p,mode,size)
        boot.update(stats)
    bilan=re.search(r"ETH3 bilan .*",serie)[0]
    hz=entier(bilan,"hz")
    boot["hz"]=hz
    boot["serie_ticks"]=entier(bilan,"serie_ticks")
    boot["ressources"]={k:entier(bilan,k) for k in ("appels_pilote","changements","reveils","cpu_ticks","idle_ticks","pages_ouverture","pages_publication")}
    boot["ressources"]["cpu_usr_sys_idle"]=ast.literal_eval(re.search(r"echantillons_cpu_usr_sys_idle=(\[[^\]]+\])",serie)[1])
    boot["ressources"]["cadres"]=list(map(int,re.search(r"ETH3 cadres avant=(\d+) apres=(\d+)",serie).groups()))
    exiger(boot["ressources"]["cadres"][0]==boot["ressources"]["cadres"][1],p)
    boot["ressources"]["pics_dma"]=int(re.search(r"RAM phase=eth3_publication [^\n]*?\bdma=(\d+)",serie)[1]) if "RAM phase=eth3_publication " in serie else None
    boot["cellules"]={}
    for i,b in enumerate(LOTS):
        n,s,q=re.search(rf"ETH3 lot={b} n=(\d+) somme_ticks=(\d+) quantiles_ticks=(\[[^\]]+\])",serie).groups()
        exiger(int(n)==1024,(p,b,n))
        cout=ast.literal_eval(re.search(rf"E3B cout lot={b} compte=(\[[^\]]+\])",serie)[1])
        exiger(len(cout)==len(COUTS),(p,b))
        rate=int(n)*b*size*8*hz/int(s)/1e6
        tx=0 if mode.startswith("rx") else rate
        rx=0 if mode.startswith("tx") else rate
        quant=ast.literal_eval(q)
        exiger(len(quant)==4 and quant==sorted(quant) and quant[0]>0,(p,b,"quantiles"))
        trames=int(n)*b
        publiees=0 if mode.startswith("rx") else trames
        recues=0 if mode.startswith("tx") else trames
        exiger(cout[28]==publiees and cout[29]==recues*(size+12),(p,b,"publications et longueurs DMA"))
        exiger(cout[4]==4*(publiees+recues)*size,(p,b,"octets des deux liaisons IPC"))
        cell=dict(n=int(n),somme_ticks=int(s),quantiles_ticks=quant,cout=cout,
                  tx_mbps=tx,rx_mbps=rx,total_mbps=tx+rx,p99_ms=1000*quant[2]/hz,
                  ipc_par_trame=sum(cout[:3])/(int(n)*b))
        if limites is not None:
            cell["pair_mbps"]=[]
            for sens,actif in enumerate((tx,rx)):
                premier,dernier=limites[sens][i]
                cell["pair_mbps"].append((int(n)*b-1)*size*8/(dernier-premier) if actif else 0.)
        if detail:
            mesures=ast.literal_eval(re.search(rf"E3B detail lot={b} mesures=(.*)",serie)[1])
            exiger(len(mesures)==len(DETAILS),p)
            cell["detail"]=mesures
        boot["cellules"][str(b)]=cell
    return boot


def resumer(series):
    lignes=[]
    for arch in ARCH:
        for mode in MODES:
            for b in LOTS:
                group={}
                for variante,rows in series.items():
                    cells=[r["cellules"][str(b)] for r in rows if r["arch"]==arch and r["mode"]==mode]
                    exiger(len(cells)==3,(arch,mode,b,variante))
                    group[variante]={k:median(c[k] for c in cells) for k in ("tx_mbps","rx_mbps","total_mbps","p99_ms","ipc_par_trame")}
                avant,apres=group["avant"],group["apres"]
                lignes.append(dict(arch=arch,mode=mode,lot=b,**group,ratio_debit=apres["total_mbps"]/avant["total_mbps"],
                                   ratio_p99=apres["p99_ms"]/avant["p99_ms"],ratio_ipc=apres["ipc_par_trame"]/avant["ipc_par_trame"]))
    return lignes


def controles(archives):
    controles=[]
    for mode in ("boite","data","ancien","choix-data","plein","veille","filtre","deces","incertain","revocation","court","segment-interrompu","recu-collecte"):
        for arch in ARCH:
            p=archives/"invariants"/mode/arch/"1"
            c=lire(p/"campagne.json")
            t=(p/"serie.log").read_text()
            exiger(c["succes"] and c["profil_verifie"] and not c["erreur_pair"],p)
            racines,boites,points,ipc_pages=re.search(r"racines=(\[[0-9, ]+\]) boites_globales=(\d+) endpoints_globaux=(\d+) ipc_pages=(\d+)",t).groups()
            exiger(ast.literal_eval(racines)==[0,4,1] and int(boites)<=8,p)
            attendu=1 if mode in ("data","choix-data","plein") else 2 if mode=="ancien" else 3
            exiger(c["transport_client"]==attendu,p)
            controles.append(dict(mode=mode,arch=arch,elf=c["elf"],succes=True,transport=c["transport_client"],
                trames_tx=c["trames_par_sens"],rx_sans_tx=c["rx_sans_tx"],rx_filtrees=c["rx_filtrees"],
                racines=ast.literal_eval(racines),boites=int(boites),endpoints=int(points),pages_ipc=int(ipc_pages),
                resultats_acteurs=re.search(r"acteurs=(\[[^\]]+\])",t)[1],
                retrait=re.search(r"ETH3 bilan [^\n]*? retrait=(\S+)",t)[1],
                sha256_serie=digest(p/"serie.log"),sha256_pcap=digest(p/"pair.pcap.gz"),
                publications=c.get("publications"),neuvieme_refuse=c.get("neuvieme_refuse"),dma=c.get("dma",c.get("domaines"))))
    duo={}
    for mode in ("port","deces"):
        p=archives/"duo"/mode/"q35"
        c=lire(p/"campagne.json")
        exiger(c["succes"] and c["commit_sources"].startswith("ff864f5"),p)
        duo[mode]={k:c[k] for k in ("elf","graine","stockage","p9_final","trames_par_sens","commit_sources")}
    return controles,duo


def veille_reelle(archives, machines):
    """Relie chaque mesure de veille à son contrôle au pair et à son ELF."""
    resultats = {}
    for variante, revision in (("avant", "cdf5681"), ("apres", "1a7de0e")):
        resultats[variante] = []
        for mode in ("veille", "filtre"):
            for arch in ARCH:
                for numero in range(1, 4):
                    dossier = archives / ("veille-" + variante) / mode / arch / str(numero)
                    campagne = lire(dossier / "campagne.json")
                    serie = (dossier / "serie.log").read_text()
                    exiger(campagne["succes"] and campagne["profil_verifie"], dossier)
                    exiger(campagne["construction"]["diagnostic"] and
                           campagne["commit_sources"].startswith(revision), dossier)
                    exiger(campagne["elf"] == digest(dossier / "novaos.elf"), dossier)
                    exiger(machine(dossier, campagne) == machines[arch], dossier)
                    exiger(campagne["rx_sans_tx"] == 16 and
                           campagne["rx_filtrees"] == (16 if mode == "filtre" else 0), dossier)
                    ligne = re.search(r"^E3B veille (.*)$", serie, re.MULTILINE)
                    exiger(ligne is not None, (dossier, "sondes de veille absentes"))
                    valeurs = {
                        nom: [int(compte), int(ticks)]
                        for nom, compte, ticks in re.findall(r"(\w+)=\[(\d+), (\d+)\]", ligne[1])
                    }
                    exiger(set(valeurs) == {"arm", "notify", "reprise", "rearmement_rx"}, dossier)
                    exiger(valeurs["arm"][0] == 2 and valeurs["notify"][0] == 1
                           and valeurs["reprise"][0] == 1, (dossier, valeurs))
                    exiger(all(valeurs[nom][1] > 0 for nom in ("arm","notify","reprise")), dossier)
                    nombre, ticks = valeurs["rearmement_rx"]
                    exiger((nombre == 0) == (ticks == 0) and (mode != "filtre" or nombre > 0), dossier)
                    resultats[variante].append(dict(
                        arch=arch, mode=mode, demarrage=numero, acquisition=campagne["acquisition"],
                        elf=campagne["elf"], sources=campagne["commit_sources"],
                        hz=entier(re.search(r"^ETH3 bilan .*$", serie, re.MULTILINE)[0], "hz"),
                        mesures=valeurs, sha256_serie=digest(dossier / "serie.log"),
                    ))
    return resultats


def echantillon(pcap):
    """Premières seize trames de chaque sens après la borne commune 5 136."""
    resultat=io.BytesIO()
    with gzip.open(pcap,"rb") as src:
        resultat.write(src.read(24))
        n=[0,0]
        while min(n)<5152:
            header=src.read(16)
            exiger(len(header)==16,pcap)
            size=struct.unpack("<IIII",header)[2]
            data=src.read(size)
            sens=int(data[:6]==NIC)
            if 5136<=n[sens]<5152:
                resultat.write(header+data)
            n[sens]+=1
    return resultat.getvalue()


def archiver(racine, destination):
    dossiers=[]
    for variant in ("avant-serie","apres-serie","apparie-avant","apparie-apres","detail-avant","detail-apres","comparaison-pair","confirmation-livraison","invariants",
                    "veille-avant","veille-apres","confirmation-notifications"):
        dossiers.extend(sorted((racine/variant).glob("*/*/*/campagne.json")))
    dossiers.extend(sorted((racine/"duo").glob("*/*/campagne.json")))
    inventaire={}
    def ajouter(tar,nom,octets):
        info=tarfile.TarInfo(nom)
        info.size=len(octets)
        tar.addfile(info,io.BytesIO(octets))
    with tarfile.open(destination,"x:gz",compresslevel=6) as archive:
        for campagne in dossiers:
            p=campagne.parent
            nom=p.relative_to(racine)
            for name in ("campagne.json","serie.log","arguments.json","fin.json","tables.json","sources.json","groupes-pair-ns.json"):
                source=p/name
                if source.is_file():
                    archive.add(source,arcname=str(nom/name),recursive=False)
            inventaire[str(nom)]={name:digest(p/name) for name in ("qemu.trace","pair.pcap.gz","novaos.elf")}
            if nom.parts[0]=="invariants" and nom.parts[1] in ("boite","segment-interrompu","recu-collecte"):
                archive.add(p/"pair.pcap.gz",arcname=str(nom/"pair.pcap.gz"))
            if nom.parts[0] in ("avant-serie","apres-serie","comparaison-pair") and nom.parts[1] in ("echo1514","historique1514") and p.name=="1":
                ajouter(archive,str(nom/"fenetre-16-par-sens.pcap"),echantillon(p/"pair.pcap.gz"))
        for source in sorted(racine.glob("images-*/*/construction.json")):
            archive.add(source,arcname=str(source.relative_to(racine)))
        for source in sorted((racine/"contrat-ff864f5").iterdir()):
            if source.suffix in (".log",".json"):
                archive.add(source,arcname=str(source.relative_to(racine)))
        for source in sorted((racine/"images-duo-ff864f5").glob("*.json")):
            archive.add(source,arcname=str(source.relative_to(racine)))
        essai=racine/"avant-cdf5681/rx1514/arm/1/campagne.json"
        archive.add(essai,arcname="mise-au-point/rx1514-arm-incomplet.json")
        ajouter(archive,"empreintes-artefacts-locaux.json",json.dumps(inventaire,indent=2).encode())


def main():
    cli=argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archives",type=Path)
    cli.add_argument("sortie",type=Path)
    cli.add_argument("--preuves",type=Path,help="Archive compacte des journaux et empreintes")
    a=cli.parse_args()
    machines={}
    def corpus(dirname,modes,k,detail=False):
        return [run(a.archives/dirname/mode/arch/str(i),mode,machines,detail)
                for mode in modes for arch in ARCH for i in range(1,k+1)]
    series={v:corpus(v+"-serie",MODES,3) for v in ("avant","apres")}
    diag={v:corpus("detail-"+v,["echo60","echo1514"],1,True) for v in ("avant","apres")}
    hist=corpus("comparaison-pair",["historique100k","mixte100k","historique1514"],3)
    for arch in ARCH:
        empreintes={tuple(r["fenetre_1514"]["empreintes"]) for r in hist+series["avant"]+series["apres"]
                    if r["arch"]==arch and "fenetre_1514" in r}
        exiger(len(empreintes)==1,(arch,"octets différents dans la fenêtre appariée"))
    attendues={"avant":"cdf568194dbbc357b8c69d92153a1b7f04505e53","apres":"ff864f5bec14315e7374375a92ca47912a0abdc5"}
    for variante,boots in series.items():
        exiger({boot["sources"] for boot in boots}=={attendues[variante]},variante)
        for arch in ARCH:
            for mode in MODES:
                exiger(len({boot["elf"] for boot in boots if boot["arch"]==arch and boot["mode"]==mode})==1,(variante,arch,mode))
    contre_epreuve={v:corpus("apparie-"+v,["echo1514"],3) for v in ("avant","apres")}
    for v,boots in contre_epreuve.items():
        exiger({r["sources"] for r in boots}=={attendues[v]},v)
        for r in boots:
            images={b["elf"] for b in series[v] if b["mode"]=="echo1514" and b["arch"]==r["arch"]}
            exiger(images=={r["elf"]}, (v,r["arch"],"image de contre-épreuve"))

    rows=resumer(series)
    resultat={r["arch"]:r["ratio_debit"]>=2 and r["ratio_p99"]<1 and r["ratio_ipc"]<1
              for r in rows if r["mode"]=="echo1514" and r["lot"]==8}
    confirmation=corpus("confirmation-livraison",["echo1514"],1)
    exiger(all(r["sources"].startswith("52d6429") for r in confirmation),"Révision de livraison")
    all_runs=[r for cat in (series,diag,contre_epreuve) for v in cat.values() for r in v]+hist+confirmation
    exiger(len({r["acquisition"] for r in all_runs})==len(all_runs),"Démarrage réutilisé")
    invariants,duo=controles(a.archives)
    veille = veille_reelle(a.archives, machines)
    ultime = corpus("confirmation-notifications", ["echo1514"], 1)
    exiger(all(r["sources"].startswith("1a7de0e") for r in ultime), "Livraison des sondes")
    acquisitions = [r["acquisition"] for r in all_runs + ultime + veille["avant"] + veille["apres"]]
    exiger(len(acquisitions) == len(set(acquisitions)), "Démarrage de veille réutilisé")
    doc=dict(invariants=invariants,stockage_q35=duo,veille=veille,confirmation_notifications=ultime,criteres="a18d23d586187bce3b94bf8a0f351d045ebe8147",profils=machines,colonnes_cout=COUTS,classes_detail=DETAILS,series=series,
             diagnostic=diag,temoins=hist,contre_epreuve=contre_epreuve,confirmation_livraison=confirmation,comparaison=rows,objectifs=resultat)
    a.sortie.write_text(json.dumps(doc,ensure_ascii=False,separators=(",",":"))+"\n")
    if a.preuves:
        archiver(a.archives,a.preuves)


if __name__=="__main__":
    main()
