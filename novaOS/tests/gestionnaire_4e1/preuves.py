"""Reconstituer la matrice 4E.1 et relire l'intégralité de la capsule."""
import argparse
from collections import Counter, defaultdict
import gzip
import hashlib
import json
import math
from pathlib import Path
import re
import sys
import subprocess
import tarfile

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "tests"))
from gestionnaire_4e1.qualifier import CAS, exiger, lignes, sha, verifier, write
sys.path.insert(0, str(NOVA / "tests/gestionnaire_4d"))
from preuves import ethernet

def lire(p):
    return json.loads(p.read_text())

def quantiles(valeurs):
    if not valeurs:
        return None
    tries=sorted(valeurs)
    def rang(p):
        return round(tries[math.ceil(len(tries)*p)-1],4)
    return dict(n=len(tries),p50=rang(.50),p95=rang(.95),p99=rang(.99),maximum=round(tries[-1],4))

def memoire(serie):
    lignes_ram=defaultdict(list)
    for v in lignes(serie, "RAM phase="):
        lignes_ram[v["phase"]].append({k:int(i) for k,i in v.items() if k!="phase"})
    # Les maxima des colonnes peuvent appartenir à des instants différents.
    def enveloppe(series):
        return {k:(min if k=="libres" else max)(v[k] for v in series) for k in series[0]}
    return {ph:enveloppe(vs) for ph,vs in lignes_ram.items()}

def verifier_chronologie(cas, serie):
    if cas in {"gestionnaire-actif","lanceur-actif","publication"}:
        segments=re.findall(r"E41 vacance role=(\d+).*?generation=(\d+).*?\n(.*?)E41 reprise phase=([\w-]+)",serie,re.S)
        exiger(len(segments)==2, "deux intervalles de vacance")
        for role,gen,contenu,phase in segments:
            exiger(phase==cas and "lecture_ecriture_flush_relecture=true" in contenu and
                   f"E41 stockage sans_controle role={role} generation={gen} " in contenu,
                   "chronologie stockage/reprise")
            if cas!="publication":
                exiger(f"E41 RX sans_controle generation={gen} " in contenu and "D4 lancement scelle" not in contenu,
                       "RX pendant la vacance")
        exiger(serie.count(" remise=1")==len(segments), "remises du reçu matériel")
    if cas=="double-deces":
        recu=lignes(serie, "E41 recu ")[0]
        exiger(re.search(r"PROC pid="+recu["pilote"]+r" niveau=EL0 .*?sortie=9.*?E41 vacance role=1 .*?E41 concurrence .*?E41 rejeu apres_deces",serie,re.S),
               "ordre des décès et du rejeu")
    if cas=="non-corroborable" or cas=="journal-perdu":
        exiger("E3C rattachement refuse" in serie and "E41 gel cause=" in serie,"gel et refus de rattachement")

def verifier_controleur(defaut,pieces,captures,boots,source,plateforme):
    from gestionnaire_4e1.controleur import verifier as verifier_defaut
    sys.path.insert(0,str(NOVA/"tests/ethernet_v1"))
    construction=lire(defaut/"image/construction.json")
    exiger(construction["patch"]==sha(defaut/"image/injection.patch"),"recette du diagnostic")
    subprocess.run(["git","diff","--quiet",construction["base"],source,"--","novaOS/kernel","novaOS/user","novaOS/libs",
                    "novaOS/abi","novaOS/Cargo.toml","novaOS/Cargo.lock","novaFS","components"],cwd=NOVA.parent,check=True)
    for nom in ("injection.patch","construction.json","construction.log"):
        pieces[f"controleur-commun/image/{nom}"]=defaut/"image"/nom
    echantillons=[]
    for i in range(1,4):
        dossier=defaut/str(i)
        c=lire(dossier/"campagne.json")
        texte=(dossier/"serie.log").read_text()
        exiger(c["succes"] and c["elf"]==construction["elf"]["arm"] and c["commit_sources"]==construction["commit"],"image du diagnostic")
        exiger(not c["sources_suivies_modifiees"] and all(c[k]==v for k,v in plateforme.items()),"lanceur ou plateforme du diagnostic")
        oracle=verifier_defaut(dossier,c,texte)
        # Les clés de domaines numériques sont représentées par des chaînes en JSON.
        exiger(json.loads(json.dumps(oracle))==c["cycle_vie"],"relecture du diagnostic")
        e={v["boot"] for v in lignes(texte,"E41 demarrage ")}
        exiger(len(e)==1 and not e & boots,"époque du diagnostic");boots.update(e)
        cap=ethernet(dossier/"pair.pcap.gz")
        exiger(cap["trames"]=={"tx":1,"rx":1},"PCAP diagnostic")
        captures[f"controleur-commun/{i}"]=cap
        echantillons.append({"boot":next(iter(e)),"elf":c["elf"],"cadres":oracle["dma_cadres"],
                             "stockage":c["volume_p9"],"domaines":oracle["iommu"]})
        for p in dossier.iterdir():
            if p.is_file() and p.name not in {"labo.tpm","novaos.elf"}:
                pieces[f"controleur-commun/{i}/{p.name}"]=p
    return {"construction":construction,"demarrages":echantillons}

def capsule(archive,temoin,hotes,defaut,sortie):
    source=lire(archive/"images/attribution/construction.json")["commit"]
    exiger(re.fullmatch("[0-9a-f]{40}",source), "révision source")
    historique=lire(NOVA/"docs/validation/gestionnaire-pilotes-phase4d/bilan.json")
    reference=historique["plateforme"]
    host=hotes.read_text()
    exiger("Révision "+source in host and host.count("tests réussis 26 ignorés 1")==2 and
           host.count("tests réussis 28 ignorés 0")==2 and host.count("3 passed; 0 failed")==2 and
           host.count("objet catalogue-pilotes")==2 and host.count("objet gestionnaire-pilotes")==2,
           "suites hôtes ou cibles nues")
    pieces={"hotes/resultats.log":hotes}
    boots=set()
    profils={}
    captures={}
    observations={}
    tous=Counter()
    for cas in CAS:
        construction=archive/"images"/cas/"construction.json"
        image=lire(construction)
        exiger(image["commit"]==source and image["reprise"]==cas, "construction "+cas)
        pieces[f"images/{cas}/construction.json"]=construction
        pieces[f"images/{cas}/construction.log"]=construction.with_name("construction.log")
        series=[]
        latences=[]
        for numero in range(1,4):
            nom=f"{cas}/{numero}"
            dossier=archive/nom
            c=lire(dossier/"campagne.json")
            local=verifier(cas,dossier)
            exiger(local==lire(dossier/"reprise.json"),"résultat du lanceur "+nom)
            exiger(c["commit_sources"]==source and not c["sources_suivies_modifiees"] and c["elf"]==image["elf"]["arm"],
                   "provenance de l'image "+nom)
            exiger(all(c[k]==v for k,v in reference.items()),"modèle ou firmware différents "+nom)
            exiger(c["volume_p9"]["verification"] and len(c["volume_p9"]["ancres"])==2,"montage P9 "+nom)
            trame=ethernet(dossier/"pair.pcap.gz")
            for sens in ("tx","rx"):
                exiger(trame["trames"].get(sens,0)==c["trames_par_sens"] and trame["octets"].get(sens,0)==c["octets_par_sens"], "PCAP "+nom)
            captures[nom]=trame
            texte=(dossier/"serie.log").read_text()
            verifier_chronologie(cas,texte)
            epochs={v["boot"] for v in lignes(texte,"E41 demarrage ")}
            exiger(len(epochs)==1 and not epochs & boots, "époque de démarrage réutilisée "+nom)
            boots.update(epochs)
            fin=lignes(texte,"D4 cycle avant=")
            physique=lignes(texte,"D4 physique phase=")
            ressources=memoire(texte)
            if numero==1:
                observations[cas]=dict(ram=ressources,journal=local["journal"],
                    selection={cle:max(int(x[cle]) for x in physique if x["phase"]=="selection")
                               for cle in ("pic","allocations")})
                lancement=[x for x in physique if x["phase"]=="lancement"]
                if lancement:
                    observations[cas]["lancement"]={cle:max(int(x[cle]) for x in lancement) for cle in ("pic","allocations")}
            if cas=="endurance":
                exiger(len(fin)==256 and len({(v["avant"],v["apres"],v["dma"],v["ipc"]) for v in fin})==1,
                       "dérive d'endurance "+nom)
                for ph in ("4d_fin_cycle", "eth3_retrait"):
                    for cle in ("processus","upile","kpile","tables","ipc","dma"):
                        exiger(len({v[cle] for v in c["cycle_vie"]["ram"][ph]})==1,"dérive "+ph+" "+cle)
            latences+=local["reprises_ms"]
            series.append(dict(boot=next(iter(epochs)),image=c["elf"],cycles=local["cycles"],pic_cadres=local["pic_cadres"],
                recuperation_ms=quantiles(local["reprises_ms"]),creations=local["creations"],trames_par_sens=c["trames_par_sens"],
                quarantaine_nic=local["nic_retenus"],dma_fin=int(fin[-1]["dma"]),ipc_fin=int(fin[-1]["ipc"]),
                attente_verrou_s=c["attente_s"]))
            tous.update(demarrages=1,cycles=local["cycles"],reprises=len(local["reprises_ms"]),
                        creations=local["creations"],trames_par_sens=c["trames_par_sens"])
            for p in dossier.iterdir():
                if p.is_file() and p.name not in {"labo.tpm","novaos.elf"}:
                    pieces[f"{nom}/{p.name}"]=p
        exiger(len({captures[f"{cas}/{i}"]["sha256_sans_temps"] for i in (1,2,3)})==1,"trames différentes pour "+cas)
        profils[cas]={"demarrages":series,"recuperation_ms":quantiles(latences)}
    baseline=[]
    for n in range(1,4):
        racine=temoin/"reset-court"/str(n)
        c=lire(racine/"campagne.json")
        exiger(c["succes"] and c["commit_sources"]==source and not c["sources_suivies_modifiees"] and
               all(c[k]==v for k,v in reference.items()),"témoin 4D à la même révision")
        texte=(racine/"serie.log").read_text()
        baseline.append(dict(pic=c["dynamique"]["pic_cadres"],selection=max(int(v["pic"]) for v in lignes(texte,"D4 physique phase=selection")),
                             ram=memoire(texte)["net_initial"],cycles=c["dynamique"]["cycles"],elf=c["elf"]))
        for nom in ("campagne.json","serie.log","pair.pcap.gz","fin.json","reset.json"):
            pieces[f"temoin-4d/{n}/{nom}"]=racine/nom
    pieces["temoin-4d/construction.json"]=temoin/"images/reset-court/construction.json"
    diagnostic=verifier_controleur(defaut,pieces,captures,boots,source,reference)
    sortie.mkdir(parents=True,exist_ok=True)
    bilan=dict(revision=source,plateforme=reference,totaux=tous,profils=profils,memoire=observations,
               volume_p9=capturer_volume(archive),temoin_4d=baseline,defaut_commun=diagnostic,
               references_4d=dict(revision=historique["reference"],selection=171,cycle=490,pression=790,surcout_image=60))
    write(sortie/"bilan.json",bilan)
    write(sortie/"captures.json",captures)
    index={nom:dict(octets=p.stat().st_size,sha256=sha(p)) for nom,p in sorted(pieces.items())}
    write(sortie/"inventaire.json",index)
    paquet=sortie/"preuves.tar.gz"
    with paquet.open("wb") as f,gzip.GzipFile(fileobj=f,mode="wb",filename="",mtime=0,compresslevel=6) as gz,tarfile.open(fileobj=gz,mode="w|") as tar:
        for nom,p in sorted(pieces.items()):
            info=tarfile.TarInfo(nom);info.size=index[nom]["octets"]
            with p.open("rb") as src:tar.addfile(info,src)
    vus=set()
    with tarfile.open(paquet,"r|gz") as tar:
        for membre in tar:
            exiger(membre.name in index and membre.name not in vus and membre.isfile(),"entrée de capsule")
            with tar.extractfile(membre) as f:
                exiger(hashlib.file_digest(f,"sha256").hexdigest()==index[membre.name]["sha256"],membre.name)
            vus.add(membre.name)
    exiger(vus==set(pieces),"membres manquants")
    (sortie/"SHA256SUMS").write_text("".join(f"{sha(sortie/n)}  {n}\n" for n in
                                             ("bilan.json","captures.json","inventaire.json","preuves.tar.gz")))
    print(json.dumps(tous),len(vus),"fichiers vérifiés")

def capturer_volume(archive):
    volumes=[lire(p)["volume_p9"] for p in sorted(archive.glob("*/[123]/campagne.json"))]
    exiger(volumes and all(v==volumes[0] for v in volumes),"volumes et ancres des démarrages")
    return volumes[0]

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    for param in ("archive","temoin","hotes","defaut","sortie"):
        p.add_argument(param,type=Path)
    a=p.parse_args()
    capsule(a.archive,a.temoin,a.hotes,a.defaut,a.sortie)
