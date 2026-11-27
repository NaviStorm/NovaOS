"""Consolide les preuves de phase 3E directement depuis les invités, les PCAP et les traces."""
import argparse
import gzip
from hashlib import file_digest
import io
import json
import os
from pathlib import Path
import re
from statistics import median
import sys
import tarfile
from types import SimpleNamespace

import cycle_vie
from bilan_cycle_vie import trames, trafic_disque_pendant_retrait
from bilan_retrait_duo import verifier_lancement, capacites_libres
from performance_bilan import exiger, lire, run
from port_qemu import digest, qualification
from rejouer_phase3e import ARM, MMIO, NOYAU, REFERENCE
import retrait_duo


def ecrire(path, data):
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False)+"\n")


def controles_arm(path, c):
    log = (path/"serie.log").read_text()
    preuve = cycle_vie.verifier(path, c, log)
    exiger(json.loads(json.dumps(preuve)) == c["cycle_vie"], path)
    n = c["trames_par_sens"]
    exiger(c["volume_relu_identique"] and c["transport_client"] == 3, path)
    fil = trames(path, "cycles-court" if c["cas"] in ("reset-court","reset-iommu-differe","mmio-differe") else c["cas"], [n,n])
    ram = preuve["ram"]
    for avant, apres in zip(ram["eth3_initial"], ram["eth3_retrait"], strict=True):
        for cle in ("libres","tables","upile","kpile","ipc","processus","utilisateur","dma"):
            exiger(avant[cle]==apres[cle], (path,cle))
    resume = dict(generations=preuve["generations"], trames=fil, bilans=preuve["bilans"],
                  arrets=preuve["arrets"], ouverture=capacites_libres(ram["eth3_ouvert"],("libres","processus","ipc","dma")),
                  tour=capacites_libres(ram["eth3_retrait"],("libres","processus","ipc","dma","tables")),
                  fin=preuve["memoire_finale"], iommu=preuve["iommu"], volume_intact=c["volume_relu_identique"])
    if c["cas"] in ("reset-iommu-absent","reset-iommu-differe"):
        categorie = "recu-arm-absent" if c["cas"].endswith("absent") else "recu-arm-differe"
        resume["activite_disque_entre_unmap"] = trafic_disque_pendant_retrait(path,categorie,len(preuve["generations"]))
    if c["cas"] in cycle_vie.RESET:
        resume["capacite_pci"] = c["capacite_pci_arm"]
    return resume


def lectures_mmio(path, cas):
    """Décode les accès CPU au BAR PCI réel et à la file sélectionnée en écriture."""
    pci = lire(path/"fin.json")["pci"][0]["devices"]
    nic, = [d for d in pci if d["slot"]==7 and d["qdev_id"]=="nic"]
    base, = [r["address"] for r in nic["regions"] if r["bar"]==4]
    motif = re.compile(r"memory_region_ops_(read|write) cpu 0 mr \S+ addr (0x\w+) value (0x\w+) size (\d+) name 'virtio-pci-common-virtio-net'")
    selection = None
    operations = []
    active = None
    marqueurs = []
    acces_reset = 0
    with (path/"qemu.trace").open() as trace:
        for ligne in trace:
            if ligne.startswith("E3E model:"):
                marqueurs.append(ligne.strip())
            if "virtio-pci-common-virtio-net" not in ligne:
                continue
            m = motif.fullmatch(ligne.rstrip())
            if not m:
                continue
            sens, adresse, valeur, taille = m.groups()
            offset = int(adresse,16) - base
            val = int(valeur,16)
            if offset==58:
                acces_reset += 1
            if sens=="write" and offset in (20,22):
                active = None
                if offset==22:
                    selection = val
            if sens=="write" and offset==58:
                exiger(val==1 and int(taille)==2 and selection in (0,1), (path,ligne))
                active = dict(file=selection, reset=[], enable=[])
                operations.append(active)
            if active is not None and sens=="read" and offset in (28,58):
                exiger(int(taille)==2,(path,ligne))
                active["enable" if offset==28 else "reset"].append(val)
    if cas in MMIO:
        valeurs = {"mmio-absent":-1,"mmio-invalide":-2,"mmio-differe":32}
        cycles = 2 if cas=="mmio-differe" else 1
        exiger(marqueurs.count(f"E3E model: TX reset executed, readback={valeurs[cas]}")==cycles,(cas,marqueurs))
        if cas=="mmio-differe":
            exiger(marqueurs.count("E3E model: TX delayed for 32 MMIO reads")==cycles,(cas,marqueurs))
    if cas=="mmio-absent":
        exiger(not operations and not any("memory_region_ops_" in a for a in lire(path/"arguments.json")),path)
    elif cas=="reset-sans-option":
        exiger(not operations and acces_reset==0 and
               "memory_region_ops_write" in lire(path/"arguments.json"),path)
    else:
        seq=[0,1] if cas=="mmio-invalide" else [0,1,0,1]
        exiger([o["file"] for o in operations]==seq,(cas,operations))
        for op in operations:
            if op["file"]==1 and cas=="mmio-invalide":
                exiger(op["reset"]==[2] and op["enable"]==[0],(cas,op))
            else:
                intermediaires = 32 if cas=="mmio-differe" and op["file"]==1 else 0
                exiger(op["reset"]==[1]*intermediaires+[0,0] and set(op["enable"])=={0},(cas,op))
    detail = cas != "mmio-absent"
    return dict(bar4=base, acces_reset=acces_reset if detail else None,
                operations=operations if detail else None, modele=marqueurs)


def comparer(archive, index):
    machines, resultat = {}, {}
    for isa in ("arm","q35"):
        variantes = {}
        for phase,revision in (("3d",REFERENCE),("3e",NOYAU)):
            cas = "echo1514-reset" if (phase,isa)==("3e","arm") else "echo1514"
            series = []
            for numero in range(1,4):
                path = archive/("echo-"+phase)/cas/isa/str(numero)
                c = verifier_lancement(path,index,archive)
                exiger(c["commit_sources"]==revision,path)
                m = run(path,"echo1514",machines)
                series.append(dict(elf=c["elf"], acquisition=c["acquisition"], cellules={
                    lot:{k:v[k] for k in ("n","tx_mbps","rx_mbps","p99_ms","ipc_par_trame")} for lot,v in m["cellules"].items()}))
            variantes[phase]=dict(series=series, medianes={lot:{k:median(s["cellules"][lot][k] for s in series)
                for k in ("tx_mbps","rx_mbps","p99_ms","ipc_par_trame")} for lot in ("1","4","8")})
        for i,(a,b) in enumerate(zip(variantes["3d"]["series"],variantes["3e"]["series"],strict=True)):
            exiger((a["acquisition"]<b["acquisition"])==(i%2==0),(isa,i))
        resultat[isa]=variantes
    return resultat


def contre_epreuve(archive, index):
    resultat, machines, empreintes = {}, [], set()
    for choix in ("offert","masque"):
        signature, mesures = {}, []
        for i in range(1,4):
            p = archive/"option-arm"/choix/"arm"/str(i)
            c = verifier_lancement(p,index,archive)
            empreintes.add(c["elf"])
            pci = c["capacite_pci_arm"]
            exiger(c["commit_sources"]==NOYAU and pci["bit_40_offert"]==(choix=="offert")
                   and pci["bit_40_negocie"]==(choix=="offert"),p)
            m = run(p,"echo1514",signature)["cellules"]["8"]
            mesures.append(dict(acquisition=c["acquisition"], **{k:m[k] for k in ("n","tx_mbps","p99_ms","ipc_par_trame")}))
        signe = signature["arm"].copy()
        if choix=="masque":
            devices = signe["-device"]
            exiger(sum(d.endswith(",queue_reset=off") for d in devices)==1, devices)
            signe["-device"]=[d.removesuffix(",queue_reset=off") for d in devices]
        machines.append(signe)
        resultat[choix]=dict(series=mesures,mediane={k:median(m[k] for m in mesures) for k in ("tx_mbps","p99_ms","ipc_par_trame")})
    exiger(len(empreintes)==1 and machines[0]==machines[1],"ELF, firmware ou options de la contre-épreuve")
    for i,(a,b) in enumerate(zip(resultat["offert"]["series"],resultat["masque"]["series"],strict=True)):
        exiger((a["acquisition"]<b["acquisition"])==(i%2==0),("ordre A/B",i))
    resultat["elf"]=empreintes.pop()
    return resultat


def stockage(archive, index):
    path=archive/"refus/q35"
    c=verifier_lancement(path,index,archive)
    wt=Path(os.environ["NOVAOS_TMPDIR"]).resolve(strict=True).parent
    temoin=wt/"pilote-virtio-net"
    sys.path[:0]=[str(temoin/"novaOS/tests"),str(temoin/"novaOS/scripts")]
    from nvme_phase2_oracle import inspect
    lecteur=temoin/"novaFS/target/aarch64-apple-darwin/release/novafs"
    graine=wt/"archives-virtio-net/finale-20261125/q35-duo-nvme-init-512-S01/qemu/namespace.raw"
    exiger(c["commit_sources"]==NOYAU and c["graine"]==digest(graine) and c["lecteur"]==digest(lecteur)
           and c["p9_final"]==digest(path/"namespace.raw"),path)
    preuve=retrait_duo.verifier(path,"duo-retrait",lecteur,inspect(lecteur,graine,512),SimpleNamespace(frames=4,error=None))
    exiger(json.loads(json.dumps(preuve))==c["stockage"],path)
    sources = lire(path/"sources.json")
    origine = wt/"archives-ethernet-phase3d/images-duo-58e3872/sources.json"
    exiger(digest(origine)=="9d6cebe0ad7c4759e6adc63844776385aaac0d6bd672dff486c9fd133fe35be4",origine)
    ref = lire(origine)
    diff = [k for k,v in ref["raccord_3d"].items() if sources["raccord_3d"][k]!=v]
    exiger(ref["audit"]==sources["audit"] and ref["modules"]==sources["modules"]
           and diff==["novaOS/kernel/src/sched/process/nvme.rs"] and
           sources["sonde_garde_sha256"]==digest(Path(__file__).with_name("sonde_garde_q35.patch")),diff)
    serie=(path/"serie.log").read_text()
    garde=c["garde"]
    exiger(serie.count("E3E garde readmission=refusee")==1 and garde["cqe_retrouvee"]
           and c["qi_finale"]["cookie"]==5 and c["qi_finale"]["contextes_finaux"]=={},garde)
    return dict(garde=garde, delta_modules=diff, qi=c["qi_finale"], stockage=preuve, trames=trames(path,"port",[4,4]))


def archiver(dest, racine, index):
    noms={"arguments.json","campagne.json","serie.log","sources.json","construction.json","bilan.json","fin.json","reset.json"}
    with (dest/"preuves.tar.gz").open("xb") as brut, gzip.GzipFile(filename="",mode="wb",fileobj=brut,mtime=0) as gz:
        with tarfile.open(mode="w",fileobj=gz) as tar:
            for nom in sorted(index):
                p=Path(nom)
                retenir = (p.name in noms or (p.parts[0]=="hote" and p.suffix==".log")
                           or (p.name=="pair.pcap.gz" and p.parts[0] in ("observations","arm-qualification","refus"))
                           or (p.parts[0] in ("refus","observations") and p.name=="qemu.trace"))
                if retenir:
                    donnees=(racine/p).read_bytes()
                    info=tarfile.TarInfo(nom)
                    info.size=len(donnees)
                    tar.addfile(info,io.BytesIO(donnees))
    with tarfile.open(dest/"preuves.tar.gz", "r:gz") as paquet:
        for piece in paquet:
            exiger(file_digest(paquet.extractfile(piece),"sha256").hexdigest()==index[piece.name],piece.name)


def consolider(root, dest):
    index={}
    result=dict(reference=REFERENCE,noyau=NOYAU,arm={},observations={},replis={})
    for groupe, liste in (("arm-qualification",(*ARM,"court")),("observations",("reset-court","reset-sans-option",*MMIO))):
        for cas in liste:
            path=root/groupe/cas/"arm/1"
            c=verifier_lancement(path,index,root)
            exiger(c["commit_sources"]==NOYAU and c["profil_verifie"],path)
            if groupe=="observations":
                result["observations"][cas]=dict(memoire=controles_arm(path,c)["fin"],**lectures_mmio(path,cas))
            else:
                result["arm"][cas]=controles_arm(path,c)
    for cas in ("choix-data","plein"):
        path=root/"arm-qualification"/cas/"arm/1"
        c=verifier_lancement(path,index,root)
        log=(path/"serie.log").read_text()
        exiger(c["commit_sources"]==NOYAU and c["transport_client"]==1 and c["profil_verifie"]
               and "cadres_fin=0" in log and c["volume_relu_identique"],cas)
        result["replis"][cas]=dict(transport="DATA",trames=trames(path,cas,[144,144]))
    result["modele_experimental"]=lire(root/"modele-mmio/construction.json")
    result["q35"]=stockage(root,index)
    result["ethernet"]=comparer(root,index)
    result["contre_epreuve_arm"]=contre_epreuve(root,index)
    result["hote"]=lire(root/"hote/bilan.json")
    exiger(result["hote"]["selecteurs"]==48 and sum(j["tests"] for j in result["hote"]["essais"])==40,"Tests hôtes")
    for d in ("images-qualifiees","image-refus","images-reference","modele-mmio","hote"):
        for path in sorted((root/d).rglob("*")):
            if path.is_file():
                index[str(path.relative_to(root))]=digest(path)
    result["demarrages"]=sum(Path(n).name=="campagne.json" for n in index)
    exiger(result["demarrages"]==37,result["demarrages"])
    dest.mkdir(parents=True,exist_ok=False)
    ecrire(dest/"bilan.json",result)
    ecrire(dest/"inventaire.json",dict(sorted(index.items())))
    archiver(dest,root,index)
    (dest/"SHA256SUMS").write_text("".join(f"{digest(dest/n)}  {n}\n" for n in ("bilan.json","inventaire.json","preuves.tar.gz")))


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive",type=lambda s:Path(s).resolve(strict=True))
    parser.add_argument("destination",type=lambda s:Path(s).resolve())
    a=parser.parse_args()
    with qualification(Path(os.environ["NOVAOS_TMPDIR"]).resolve(strict=True).parent, {}):
        consolider(a.archive, a.destination)
