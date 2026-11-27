"""Reconstituer la matrice 4F, les mesures et la capsule vérifiable depuis les démarrages."""
import argparse
from collections import Counter
from functools import cache
import gzip
import hashlib
import json
from pathlib import Path
import re
import statistics
import subprocess
import sys
import tarfile

from qualifier import BASE, CAS, NOVA, controler, elf, sha, write
from double import verifier as verifier_double
from comparaison import verifier as verifier_ordre
from ethernet_v1.performance_bilan import run as mesure
from ethernet_v1 import cycle_vie

DOUBLE = {
    "nominal": ("port","duo-retrait","duo-differe","duo-absent"),
    "deces-observe": ("duo-differe",),
    "lanceur-observe": ("duo-retrait",),
    "reconciliation": ("duo-differe",),
    "gestionnaire-actif": ("port",),
    "lanceur-actif": ("port",),
}
FICHIERS = ("campagne.json","reprise.json","serie.log","arguments.json","entrees.json","fin.json",
            "reset.json","reset-mtree.txt","fin-mtree.txt","sources.json","tables.json","qemu.trace.gz","trame-tx.bin","pair.pcap.gz")
def lire(path):
    return json.loads(path.read_text())

@cache
def empreinte_lanceur(revision):
    source = subprocess.check_output(["git", "-C", NOVA.parent, "show",
                                      revision+":novaOS/tests/ethernet_v1/port_qemu.py"])
    return hashlib.sha256(source).hexdigest()


def bornes(values):
    if not values: return None
    return dict(min=min(values),mediane=statistics.median(values),max=max(values))

def quantiles(values):
    seq=sorted(values)
    return {str(p):seq[(len(seq)*p+99)//100-1] for p in (50,95,99)} if seq else None

def ram(serie,phase):
    expressions=re.findall(rf"^RAM phase={phase} (.*)$",serie,re.M)
    return [{k:int(v) for k,v in re.findall(r"(\w+)=(\d+)",ligne)} for ligne in expressions]

def ressources(serie):
    phases = ("net_initial", "4d_controle", "4d_acteurs_attribues", "eth3_ouvert",
              "eth3_publication", "eth3_retrait", "4d_fin_cycle")
    resultat = {phase: ram(serie, phase) for phase in phases}
    for nom, motif in (("tas", "tas"), ("piles", "pile"), ("controle", "(?:manager|pair)")):
        resultat[nom] = re.findall(rf"^D4 {motif} (.*)$", serie, re.M)
    resultat["journal"] = re.findall(r"^E41 journal (.*)$", serie, re.M)
    return resultat


def cout_controle(serie):
    # Les intervalles de contrôle sont mesurés sur la même horloge que le RTT.
    hz = int(re.search(r"ETH3 bilan .*? hz=(\d+)", serie)[1])
    result = {}
    for ligne, cle in (("manager", "ticks_controle"), ("lancement scelle", "ticks"),
                       ("activation", "publication_open_ticks"), ("fermeture", "restitution_ticks")):
        match = re.search(rf"^D4 {ligne} .*?\b{cle}=(\d+)", serie, re.M)
        if match:
            result[ligne] = int(match[1]) * 1000 / hz
    return result


def cellules(dossiers, machines):
    boots=[]
    for p in dossiers:
        d=mesure(p,"echo1514",machines)
        cells={}
        for lot,c in d["cellules"].items():
            cells[lot]={"tx_mbps":c["tx_mbps"],"rx_mbps":c["rx_mbps"],"ipc_tx":c["ipc_par_trame"],
                        **{k:1000*v/d["hz"] for k,v in zip(("p50_ms","p95_ms","p99_ms"),c["quantiles_ticks"])}}
        boots.append(dict(revision=d["sources"],elf=d["elf"],lot=cells,hz=d["hz"],
                          empreintes=d["fenetre_1514"]["empreintes"],
                          trames=[d["trames_tx"],d["trames_rx"]]))
    return dict(demarrages=boots,medianes={lot:{k:statistics.median(b["lot"][lot][k] for b in boots)
        for k in boots[0]["lot"][lot]} for lot in ("1","4","8")})

def compacte(sortie, pieces):
    index={n:dict(sha256=sha(p),octets=p.stat().st_size) for n,p in sorted(pieces.items())}
    paquet=sortie/"preuves.tar.gz"
    with paquet.open("wb") as stream,gzip.GzipFile(filename="",fileobj=stream,mode="wb",mtime=0,compresslevel=5) as gz,tarfile.open(fileobj=gz,mode="w|") as tar:
        for n,info in index.items():
            item=tarfile.TarInfo(n)
            item.size=info["octets"]
            with pieces[n].open("rb") as fp:
                tar.addfile(item,fp)
    write(sortie/"inventaire.json", index)
    return len(index)


def verifier_capsule(sortie):
    noms = ("bilan.json", "inventaire.json", "preuves.tar.gz")
    lignes = (sortie/"SHA256SUMS").read_text().splitlines()
    if lignes != [f"{sha(sortie/n)}  {n}" for n in noms]:
        raise ValueError("Sommes de la capsule")
    index = lire(sortie/"inventaire.json")
    observes = set()
    with tarfile.open(sortie/"preuves.tar.gz", "r|gz") as tar:
        for item in tar:
            if not item.isfile() or item.name in observes or item.name not in index:
                raise ValueError(("Membre inattendu", item.name))
            attendu = index[item.name]
            with tar.extractfile(item) as fp:
                if item.size != attendu["octets"] or hashlib.file_digest(fp, "sha256").hexdigest() != attendu["sha256"]:
                    raise ValueError(("Empreinte ou taille", item.name))
            observes.add(item.name)
    if observes != set(index):
        raise ValueError("Inventaire incomplet")
    return len(observes)


def main():
    cli=argparse.ArgumentParser(description=__doc__)
    if len(sys.argv) == 3 and sys.argv[1] == "--verifier":
        print(f"{verifier_capsule(Path(sys.argv[2]))} pièces vérifiées.")
        return
    cli.epilog = "Vérification autonome du résultat : preuves.py --verifier DOSSIER"
    for key in ("simple","double","endurance_double","arm","arm_reprise","hotes","sortie"):
        cli.add_argument(key,type=lambda s:Path(s).resolve())
    cli.add_argument("--comparaison", type=Path, required=True,
                     help="Six paires alternées avec les mêmes ELF Q35")
    cli.add_argument("--pression", type=Path, help="Archive distincte du cas Q35 à huit boîtes")
    cli.add_argument("--double-actif", type=Path, required=True,
                     help="Archive des deux profils doubles avec témoin des CQE")
    cli.add_argument("--objets-arm", type=Path, required=True,
                     help="Dossier de sortie Cargo contenant les cinq ELF utilisateurs ARM")
    a=cli.parse_args()
    a.sortie.mkdir(parents=True,exist_ok=True)
    pieces={}
    etats={}
    memoire={}
    epochs=set()
    images={}
    controle={}
    q35_modeles=set()
    def ajouter(n,p):
        if p.exists(): pieces[n]=p
    def pieces_boot(n,p):
        obligatoires = ("campagne.json", "serie.log", "arguments.json", "pair.pcap.gz", "reset.json", "fin.json")
        if any(not (p/fichier).is_file() for fichier in obligatoires):
            raise ValueError(("Capture de démarrage incomplète", n))
        if not any((p/fichier).is_file() for fichier in ("qemu.trace", "qemu.trace.gz")):
            raise ValueError(("Trace QEMU absente", n))
        for fichier in FICHIERS: ajouter(f"{n}/{fichier}",p/fichier)
        # Le lanceur peut aussi avoir conservé la trace sans compression.
        trace=p/"qemu.trace"
        if trace.is_file() and not (p/"qemu.trace.gz").exists():
            ajouter(f"{n}/qemu.trace",trace)
    def recoupe(p, verifier, *args):
        doc = verifier(*args, enregistrer=False)
        if doc is not None and json.loads(json.dumps(doc)) != lire(p/"reprise.json"):
            raise ValueError(("Désaccord avec le relevé initial", p))
    def enregistrer(c,boot):
        if boot in epochs: raise ValueError("Époque utilisée deux fois")
        epochs.add(boot)
        q35_modeles.add(c["modele"])
    for cas in CAS:
        source = a.pression if cas == "pression" and a.pression else a.simple
        stats = []
        lat = []
        admissions = []
        retraits = []
        construction = lire(source/"images"/cas/"construction.json")
        for n in range(1,4):
            p = source/"simple"/cas/str(n)
            c = lire(p/"campagne.json")
            recoupe(p,controler,cas,p)
            if (c["elf"] != construction["elf"]["q35"] or c["commit_sources"] != construction["commit"]
                    or c["sha256_lanceur"] != empreinte_lanceur(c["commit_lanceur"])):
                raise ValueError(("Provenance Q35", cas))
            pieces_boot(f"simple/{cas}/{n}",p)
            serie = (p/"serie.log").read_text()
            materiel = cycle_vie.verifier(p, c, serie)
            if json.loads(json.dumps(materiel)) != c["cycle_vie"]:
                raise ValueError(("Désaccord de relecture des cycles VT-d", cas, n))
            if cas=="statique":
                if n == 1:
                    memoire[cas] = ressources(serie)
                stats.append(dict(cycles=1,trames=c["trames_par_sens"],revision=c["commit_sources"]))
                continue
            j = lire(p/"reprise.json")
            enregistrer(c, j["boot"])
            for ligne in j["reprises"]:
                ticks, hz = re.search(r"ticks=(\d+) hz=(\d+)", ligne).groups()
                lat.append(1000 * int(ticks) / int(hz))
            for phase,vals in (("admission",admissions),("retrait",retraits)):
                vue=re.findall(rf"F4 physique phase={phase} avant=(\d+) minimum=(\d+) apres=(\d+) pic=(\d+)",serie)
                if len(vue)!=len(j["recus"]): raise ValueError((cas,phase))
                vals.extend(int(x[-1]) for x in vue)
            qc=c["cycle_vie"]
            stats.append(dict(boot=j["boot"],revision=c["commit_sources"],cycles=len(j["recus"]),
                creations=len(j["recus"]),trames=c["trames_par_sens"],reprises=len(j["reprises"]),
                dma=int(j["ressources"][-1][-1]),libres=int(j["ressources"][-1][2]),epingles=qc["memoire_finale"],
                bouclages_qi=c["preuve_qi"]["cookie"]*48//4096))
            if n==1 and cas in ("nominal","concurrence","reconciliation","ethernet"):
                memoire[cas] = ressources(serie)
            if cas in {"nominal", "ethernet"}:
                controle.setdefault(cas, []).append(cout_controle(serie))
        if cas in ("statique", "ethernet", "nominal"):
            images[cas] = construction["noyau"]
        etats[cas] = dict(boots=stats, reprise_ms=bornes(lat), echantillons_reprise=len(lat),
                         quantiles_reprise_ms=quantiles(lat), pic_admission=bornes(admissions),
                         pic_retrait=bornes(retraits))
        ajouter(f"images/simple/{cas}/construction.json",source/"images"/cas/"construction.json")
    duos = {}
    temoin_p9 = BASE/"pilote-virtio-net"
    sys.path.insert(0, str(temoin_p9/"novaOS/tests"))
    from nvme_phase2_oracle import inspect
    lecteur = temoin_p9/"novaFS/target/aarch64-apple-darwin/release/novafs"
    empreinte_lecteur = sha(lecteur)
    for scenario,caslist in list(DOUBLE.items())+[("endurance",("duo-cycles",))]:
        for cas in caslist:
            groupe="nominal" if scenario=="endurance" else scenario
            source = a.double
            if scenario == "endurance":
                source = a.endurance_double
            elif scenario in {"gestionnaire-actif", "lanceur-actif"}:
                source = a.double_actif
            boots = []
            service = {}
            construction = lire(source/("duo-"+groupe)/"sources.json")
            for n in range(1,4):
                p=source/"double"/groupe/cas/str(n)
                recoupe(p,verifier_double,p,cas,groupe)
                c = lire(p/"campagne.json")
                j = lire(p/"reprise.json")
                enregistrer(c, j["boot"])
                pieces_boot(f"double/{groupe}/{cas}/{n}",p)
                # Un volume fictif par scénario permet de vérifier indépendamment
                # les deux ancres, leur génération et le contenu du fichier publié.
                if n == 1:
                    ajouter(f"double/{groupe}/{cas}/{n}/namespace.raw", p/"namespace.raw")
                if c["elf"] != construction["elf"][cas] or c["commit_sources"] != construction["port"]:
                    raise ValueError(("Provenance du noyau double", groupe, cas))
                stockage = c["stockage"]
                if c["lecteur"] != empreinte_lecteur:
                    raise ValueError("Empreinte du lecteur NovaFS")
                p9 = inspect(lecteur, p/"namespace.raw", 512)
                if json.loads(json.dumps(p9)) != stockage["p9"]:
                    raise ValueError(("Désaccord de relecture du volume P9", groupe, cas, n))
                serie = (p/"serie.log").read_text()
                if n == 1 and groupe == "nominal" and cas == "duo-retrait":
                    images["double_retrait"] = elf(p/"novaos.elf")
                    memoire["double_retrait"] = ressources(serie)
                ops=re.findall(r"^E3D CQE op=(READ|WRITE|FLUSH) cid=\d+ ticks=(\d+)",serie,re.M)
                # Le compteur est étalonné par le courtier de stockage.
                for tour in stockage.get("cycles",[]):
                    for op, _, us in tour["latences_us"]:
                        service.setdefault(op, []).append(us)
                fins = [tour["ram"] for tour in stockage.get("cycles", [])]
                compteurs = {cle: bornes([tour[cle] for tour in fins])
                             for cle in ("libres", "processus", "ipc", "dma")} if fins else {}
                boots.append(dict(boot=j["boot"],revision=c["commit_sources"],cycles=j.get("cycles",1),
                    fin_cycles=compteurs,generation_p9=stockage["p9"]["generation"],
                    trames=c["trames_par_sens"],p9=c["p9_final"],fichier=stockage["p9"]["sha256_fichier"],
                    processus=stockage.get("processus_distincts"),conserves=stockage.get("cadres_nic_retenus",0),
                    garde=c.get("garde"),cles_qi=c.get("qi_finale"),operations=dict(Counter(o for o,_ in ops)),
                    apres_wait=stockage.get("E_S_apres_WAIT_NIC"),instantane=c.get("nvme_actif"),
                    pics=re.findall(r"F4 physique phase=(admission|retrait) .*?pic=(\d+)",serie),
                    cqe_sans_controle=j.get("cqe_sans_controle", []),
                    remplacements=j.get("remplacements",[])))
            duos[f"{groupe}/{cas}"] = dict(
                boots=boots,
                latences_stockage_us={k: quantiles(v) for k, v in service.items()},
                echantillons_stockage={k: len(v) for k, v in service.items()},
            )
            ajouter(f"images/double/{scenario}/sources.json",source/("duo-"+groupe)/"sources.json")
    machines={}
    def trois(path): return [path/str(n) for n in range(1,4)]
    perfs={k:cellules(trois(a.simple/"simple"/k),machines) for k in ("statique","ethernet")}
    ordre = verifier_ordre(a.comparaison)
    ordre_fichier = a.comparaison/"ordre.json"
    if ordre != lire(ordre_fichier):
        raise ValueError("Ordre ou provenance des paires")
    ajouter("comparaison/ordre.json", ordre_fichier)
    for cas in ("statique", "ethernet"):
        serie = [a.comparaison/cas/str(n) for n in range(1,7)]
        perfs[cas+"_apparie"] = cellules(serie, machines)
        attendue = perfs[cas]["demarrages"][0]["elf"]
        for p in serie:
            c = lire(p/"campagne.json")
            if c["elf"] != attendue or c["sha256_lanceur"] != empreinte_lanceur(c["commit_lanceur"]):
                raise ValueError(("Dérive d’image ou du lanceur", cas, p))
            recoupe(p, controler, cas, p)
            if cas == "ethernet":
                enregistrer(c, lire(p/"reprise.json")["boot"])
            pieces_boot(f"comparaison/{cas}/{p.name}", p)
    deltas = []
    for fixe, dynamique in zip(perfs["statique_apparie"]["demarrages"], perfs["ethernet_apparie"]["demarrages"]):
        deltas.append({lot: {k: 100*(dynamique["lot"][lot][k]/fixe["lot"][lot][k]-1)
                             for k in ("tx_mbps", "p99_ms", "ipc_tx")} for lot in ("1", "4", "8")})
    perfs["arm_4e1"]=cellules(trois(a.arm/"ethernet"),machines)
    arm_boots = set()
    for n,p in enumerate(trois(a.arm/"ethernet"),1):
        c = lire(p/"campagne.json")
        serie = (p/"serie.log").read_text()
        boot, = set(re.findall(r"E41 demarrage boot=([0-9a-f]{32})", serie))
        arm_boots.add(boot)
        if not c["volume_p9"] or c["elf_relu_esp"] != c["elf"]:
            raise ValueError("Origine ARM ou volume P9")
        if n == 1:
            images["arm_echo"] = elf(p/"novaos.elf")
            noyau = (p/"novaos.elf").read_bytes()
            acteurs = {}
            for nom in ("gestionnaire.elf", "pair.elf", "net_preparation.bin", "ethernet_port.bin", "ethernet_client.bin"):
                fichier = a.objets_arm/nom
                debut = noyau.find(fichier.read_bytes())
                if debut < 0:
                    raise ValueError(("ELF ARM absent du noyau effectivement démarré", nom))
                acteurs[nom] = {**elf(fichier), "offset_noyau": debut}
            images["arm_echo"]["acteurs"] = acteurs
            memoire["arm_echo"] = ressources(serie)
        controle.setdefault("arm_echo", []).append(cout_controle(serie))
        pieces_boot(f"arm/ethernet/{n}",p)
    for n,p in enumerate(trois(a.arm_reprise/"deces-observe"),1):
        c=lire(p/"campagne.json")
        if not c["succes"] or not lire(p/"reprise.json")["reprises_ms"]: raise ValueError("Régression ARM")
        serie = (p/"serie.log").read_text()
        boot, = set(re.findall(r"E41 demarrage boot=([0-9a-f]{32})", serie))
        arm_boots.add(boot)
        pieces_boot(f"arm/deces-observe/{n}",p)
    if len(arm_boots) != 6 or epochs & arm_boots:
        raise ValueError("Époques ARM et Q35 non distinctes")
    for n,path in (("ethernet",a.arm/"images/ethernet-4f"),("deces-observe",a.arm_reprise/"images/deces-observe")):
        ajouter(f"images/arm/{n}/construction.json",path/"construction.json")
    archives = (
        ("3c_temoin_de_3d", BASE/"archives-ethernet-phase3d/echo-temoin/echo1514/q35", 5),
        ("3e", BASE/"archives-ethernet-phase3e/echo-3e/echo1514/q35", 3),
    )
    for nom, archive, nombre in archives:
        dossiers = [archive/str(n) for n in range(1, nombre + 1)]
        perfs["q35_historique_"+nom] = cellules(dossiers, machines)
        for dossier in dossiers:
            ajouter(f"historique/{nom}/{dossier.name}/campagne.json", dossier/"campagne.json")
    empreintes={tuple(b["empreintes"]) for v in perfs.values() for b in v["demarrages"]}
    if len(empreintes)!=1: raise ValueError("Les vecteurs Ethernet diffèrent")
    journal = a.hotes.read_text()
    for arch in ("aarch64-apple-darwin", "x86_64-apple-darwin"):
        for paquet, nombre in (("catalogue-pilotes",26), ("gestionnaire-pilotes",28)):
            if f"{paquet} {arch} tests réussis {nombre} " not in journal:
                raise ValueError(("Tests hôtes absents", arch, paquet))
    if journal.count("3 passed; 0 failed") != 2 or "48 sélecteurs" not in journal:
        raise ValueError("Rapport de non-régression incomplet")
    ajouter("hotes.log",a.hotes)
    write(a.sortie/"bilan.json",dict(simple=etats,double=duos,memoire=memoire,ethernet=perfs,
        modeles_q35=sorted(q35_modeles),machines=machines,empreintes_pair=list(empreintes.pop()),
        images=images,controle_ms=controle,comparaison=dict(ordre=ordre,ecarts_paires_pct=deltas),
        boots_dynamiques_q35=len(epochs),boots_arm=len(arm_boots)))
    nombre=compacte(a.sortie,pieces)
    (a.sortie/"SHA256SUMS").write_text("".join(f"{sha(a.sortie/n)}  {n}\n" for n in ("bilan.json","inventaire.json","preuves.tar.gz")))
    if verifier_capsule(a.sortie) != nombre:
        raise ValueError("Nombre de pièces relues")
    print(f"{nombre} pièces relues, {len(epochs)} démarrages Q35 dynamiques distincts.")

if __name__=="__main__":
    main()
