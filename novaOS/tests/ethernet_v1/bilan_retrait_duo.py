"""Consolide la phase 3D depuis les journaux bruts, les images et les PCAP archivés."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
from statistics import median
import subprocess
import sys
import tarfile
from types import SimpleNamespace

from bilan_cycle_vie import trames, verifier_cycle, trafic_disque_pendant_retrait
import cycle_vie
from performance_bilan import COUTS, run
from port_construire_q35 import AUDIT, NOVA, git
from port_qemu import digest
import retrait_duo as duo

BASE = "0169c75148c8359c9235f3f19359133a0e002115"
FINAL = "58e387267ea900f18e4c0620a4b737721ac3eb39"
FICHIERS = {"arguments.json", "campagne.json", "serie.log", "reset.json", "fin.json", "sources.json",
            "construction.json", "bilan.json", "groupes-pair-ns.json", "tables.json"}


def ecrire(path, valeur):
    path.write_text(json.dumps(valeur, indent=2, ensure_ascii=False) + "\n")


def capacites_libres(lignes, cles):
    return {k: [min(v[k] for v in lignes), max(v[k] for v in lignes)] for k in cles}


def verifier_lancement(path, index, racine):
    c = json.loads((path/"campagne.json").read_text())
    args = json.loads((path/"arguments.json").read_text())
    duo.exiger(c["succes"] and c["elf"] == digest(path/"novaos.elf") and c["modele"] == digest(Path(args[0])),
               f"Image ou modèle différent de la campagne {path}")
    for f in sorted(path.iterdir()):
        if f.is_file():
            index[str(f.relative_to(racine))] = digest(f)
    return c


def mesures_stockage(path):
    log = (path/"serie.log").read_text()
    hz = int(re.search(r"NET Q35 L2 GSI=23 .*? hz=(\d+)", log)[1])
    retour = {}
    for op in ("READ","WRITE","FLUSH"):
        t = sorted(int(v)*1e6/hz for v in re.findall(rf"E3D CQE op={op} cid=\d+ ticks=(\d+)", log))
        duo.exiger(t, f"Echantillons {op}")
        retour[op] = dict(n=len(t), p50=median(t), p95=t[(95*len(t)+99)//100-1], max=t[-1])
    return retour


def disposition(elf):
    sections = subprocess.check_output(["llvm-readelf","-SW",elf],text=True)
    m = re.search(r"\[\s*\d+\]\s+\.text\s+PROGBITS\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)",sections,re.I)
    duo.exiger(m, "Section .text ELF64")
    base,offset,taille=(int(v,16) for v in m.groups())
    code = elf.read_bytes()[offset:offset+taille]
    symboles = subprocess.check_output(["llvm-nm","-S","--demangle",elf],text=True)
    resultat = dict(text_sha256=hashlib.sha256(code).hexdigest(),text_octets=taille)
    for adresse,longueur,nom in re.findall(r"^([a-f0-9]+) ([a-f0-9]+) t <novaos::hardware::block::net_trames::Port>::(demande|clore_v1)$",symboles,re.M):
        va,n=int(adresse,16),int(longueur,16)
        duo.exiger(base<=va and va+n<=base+taille,nom)
        resultat[nom]=dict(adresse=hex(va),octets=n,sha256=hashlib.sha256(code[va-base:va-base+n]).hexdigest())
    duo.exiger("demande" in resultat,"Courtier dans le code invité")
    return resultat


def paquet(dest, archive, inventaire):
    with (dest/"preuves.tar.gz").open("xb") as fichier, gzip.GzipFile(fileobj=fichier,filename="",mtime=0) as flux:
        with tarfile.open(mode="w",fileobj=flux) as tar:
            for nom in sorted(inventaire):
                rel = Path(nom)
                inclure = rel.name in FICHIERS or (rel.parts[0]=="hote" and rel.suffix==".log")
                # Les traces des deux contrôles QTest permettent de relire la séquence matérielle sans VM.
                if not inclure and not (rel.parts[0]=="instantanes-finaux" and rel.name=="qemu.trace"):
                    continue
                data = (archive/rel).read_bytes()
                tarinfo = tarfile.TarInfo(nom)
                tarinfo.size = len(data)
                tar.addfile(tarinfo,io.BytesIO(data))
    with tarfile.open(dest/"preuves.tar.gz", "r:gz") as tar:
        for membre in tar:
            duo.exiger(hashlib.file_digest(tar.extractfile(membre),"sha256").hexdigest()==inventaire[membre.name],
                       f"Empreinte dans le paquet {membre.name}")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=lambda x: Path(x).resolve(strict=True))
    p.add_argument("destination", type=lambda x: Path(x).resolve())
    a = p.parse_args()
    archive, dest = a.archive, a.destination
    dest.mkdir(parents=True, exist_ok=False)
    worktrees = archive.parent
    temoin = worktrees/"pilote-virtio-net"
    lecteur = temoin/"novaFS/target/aarch64-apple-darwin/release/novafs"
    sys.path[:0] = [str(temoin/"novaOS/tests"), str(temoin/"novaOS/scripts")]
    from nvme_phase2_oracle import inspect
    source = worktrees/"archives-virtio-net/finale-20261125/q35-duo-nvme-init-512-S01/qemu/namespace.raw"
    avant = inspect(lecteur, source, 512)
    inventaire = {}
    bilan = dict(phase3c=BASE, image_duo=FINAL, audit=AUDIT, lecteur=digest(lecteur), graine=digest(source),
                 duo={}, arm={}, latences={}, ethernet={}, instantanes={})
    qualif = archive/("qualification-"+FINAL[:7])
    for cas in [*duo.CAS,"deces"]:
        path = qualif/cas/"q35"
        c = verifier_lancement(path,inventaire,archive)
        duo.exiger(c["commit_sources"]==FINAL and c["lecteur"]==bilan["lecteur"] and c["graine"]==bilan["graine"]
                   and c["p9_final"] == digest(path/"namespace.raw"), cas)
        frames = c["trames_par_sens"]
        if cas=="deces":
            from port_q35_stockage import verifier
        else:
            verifier = duo.attente_commune if cas.startswith("duo-wait-") else duo.verifier
        obtenu = verifier(path,cas,lecteur,avant,SimpleNamespace(error=None,frames=frames))
        brut = dict(c["stockage"])
        relu = json.loads(json.dumps(obtenu))
        # Les premiers lanceurs englobaient le créneau suivant la publication,
        # avant le premier armement PCI. La trace brute décide du décompte publié.
        if "E_S_apres_BME_NIC" in relu:
            duo.exiger(relu["E_S_apres_BME_NIC"] <= brut["E_S_apres_BME_NIC"], "Fenêtre de désarmement")
            brut["E_S_apres_BME_NIC"] = relu["E_S_apres_BME_NIC"]
        duo.exiger(relu==brut, f"Relecture stockage {cas}")
        mode = {"duo-cycles":"cycles","duo-reouverture":"duo-reouverture"}.get(cas,"port")
        fil = trames(path,mode,[frames,frames])
        ligne = dict(elf=c["elf"],captures=fil,modele=c["modele"],p9=obtenu["p9"],
                     qi=c.get("qi_finale",c.get("qi_incertaine",{})),source_banc=c["revision_banc"])
        if "cycles" in obtenu:
            series = obtenu["cycles"]
            ligne.update(cycles=len(series),operations=sum(e["operations"] for e in series),
                         processus_distincts=obtenu["processus_distincts"],quarantaine_nic=obtenu["cadres_nic_retenus"],
                         ressources=capacites_libres([e["ram"] for e in series],("libres","ipc","processus","dma","tables","heap")),
                         apres_desarmement=obtenu["E_S_apres_BME_NIC"],apres_wait=obtenu["E_S_apres_WAIT_NIC"],attentes=obtenu["attentes"])
        else:
            ligne.update(operations=obtenu["operations"],quarantaine_nic=26)
        bilan["duo"][cas]=ligne
    for cas in ("duo-differe","duo-absent"):
        c=verifier_lancement(archive/"instantanes-finaux"/cas/"q35",inventaire,archive)
        bilan["instantanes"][cas]=dict(qtest=c["nvme_actif"],source=c["commit_sources"])
    for cas in ("cycles-256","court","rattrapage","recu-arm-differe","recu-arm-absent"):
        serie = "arm-"+FINAL[:7]
        path = archive/serie/cas/"arm/1"
        c = verifier_lancement(path,inventaire,archive)
        log = (path/"serie.log").read_text()
        preuve = cycle_vie.verifier(path,c,log)
        duo.exiger(json.loads(json.dumps(preuve))==c["cycle_vie"], f"Relecture ARM {cas}")
        cycles = [(int(n),int(g),int(d)) for n,g,d in re.findall(r"E3C cycle=(\d+) generation=(\d+) disque_lisible=true dma_residents=(\d+)",log)]
        ram = capacites_libres(preuve["ram"]["eth3_retrait"],("libres","ipc","processus","dma","tables","heap"))
        if cas=="cycles-256":
            duo.exiger(len(cycles)==256 and [n for n,_,_ in cycles]==list(range(1,257))
                       and [g for _,g,_ in cycles]==preuve["generations"] and {d for _,_,d in cycles}=={16}
                       and all(lo==hi for lo,hi in ram.values()), "Stabilité ARM par cycle")
        duo.exiger(c["commit_sources"]==FINAL, "Révision ARM")
        fil = trames(path,"cycles" if cas=="cycles-256" else cas,[c["trames_par_sens"],c["trames_par_sens"]+c["rx_injectees_perf"]])
        operations = trafic_disque_pendant_retrait(path,cas,len(preuve["generations"])) if cas in ("recu-arm-differe","recu-arm-absent") else []
        bilan["arm"][cas]=dict(cycles=len(preuve["generations"]),generations=[preuve["generations"][i] for i in (0,-1)],
                ressources_acteurs=ram,dma_apres_cycle=sorted({d for _,_,d in cycles}),fin=preuve["memoire_finale"],
                E_S_reconciliation=operations,iommu=preuve["iommu"],fil=fil,source=c["commit_sources"],elf=c["elf"],modele=c["modele"])
    cap=verifier_lancement(archive/"capacite-arm/court/arm/1",inventaire,archive)
    bilan["capacite_pci_arm"]=cap["capacite_pci_arm"]
    bilan["transports"]={}
    machines_transport={}
    for isa in ("arm","q35"):
        for cas in ("choix-data","plein"):
            path=archive/("transport-"+FINAL[:7])/cas/isa/"1"
            verifier_lancement(path,inventaire,archive)
            bilan["transports"][f"{isa}/{cas}"]=verifier_cycle(path,FINAL,machines_transport)
    for cas in ("deces","duo-retrait","duo-differe"):
        boots=[]
        for serie in ("qualification-"+FINAL[:7], *(f"stockage-final-{n}" for n in range(2,6))):
            path = archive/serie/cas/"q35"
            c = verifier_lancement(path,inventaire,archive)
            boots.append(mesures_stockage(path))
            duo.exiger(c["commit_sources"]==FINAL and c["stockage"].get("p9",{})["etat"]=="nouveau", "Témoin de latence")
        bilan["latences"][cas]=dict(boots=boots,mediane_us={op:{k:median(m[op][k] for m in boots)
                for k in ("p50","p95")} for op in boots[0]})
    machines={}
    for variante in ("ancien3b","ancien3c","temoin","froid"):
        boots=[]
        for numero in range(1,6):
            path=archive/("echo-"+variante)/"echo1514/q35"/str(numero)
            c = verifier_lancement(path,inventaire,archive)
            mesure = run(path,"echo1514",machines)
            cell = mesure["cellules"]["8"]
            boots.append(dict(elf=c["elf"],source=c["commit_sources"],n=cell["n"],acquisition=c["acquisition"],
                tx_mbps=cell["tx_mbps"],p99_ms=cell["p99_ms"],ipc_par_trame=cell["ipc_par_trame"],
                cout_par_lot={k:cell["cout"][COUTS.index(k)]/cell["n"] for k in
                              ("attentes_bloquees","changements_contexte","reveils","publications")}))
        disposition_elf = disposition(archive/("echo-"+variante)/"echo1514/q35/1/novaos.elf")
        bilan["ethernet"][variante]=dict(boots=boots,disposition=disposition_elf,mediane={k:median(m[k] for m in boots)
                                          for k in ("tx_mbps","p99_ms","ipc_par_trame")})
    for numero in range(5):
        ordre = sorted(("temoin","froid"),key=lambda v:bilan["ethernet"][v]["boots"][numero]["acquisition"])
        duo.exiger(ordre==(["temoin","froid"] if numero%2==0 else ["froid","temoin"]), "Ordre A/B")
    bilan["machine_echo"]=machines
    controles=bilan["ethernet"]["temoin"]["boots"]
    essais=bilan["ethernet"]["froid"]["boots"]
    bilan["effet_cloture"]={k:sum((b[k]>a[k] if k=="tx_mbps" else b[k]<a[k]) for a,b in zip(controles,essais))
                            for k in ("tx_mbps","p99_ms")}
    for boot in controles+essais:
        duo.exiger(boot["n"]==1024, "Taille de la cellule de mesure")
    bilan["hote"]=json.loads((archive/"hote/bilan.json").read_text())
    for p in (archive/"hote").iterdir():
        inventaire[str(p.relative_to(archive))]=digest(p)
    for p in sorted(archive.glob("images*/**/*")):
        if p.is_file() and p.name in ("sources.json","construction.json"):
            inventaire[str(p.relative_to(archive))]=digest(p)
    for nom in ("novaOS/abi","novaOS/Cargo.toml","novaOS/kernel/Cargo.toml","novaFS","components","distribution"):
        duo.exiger(git("rev-parse",BASE+":"+nom)==git("rev-parse","HEAD:"+nom), nom)
    bilan["objets_conserves"]={nom:git("rev-parse",BASE+":"+nom).decode().strip() for nom in
        ("novaOS/abi","novaOS/Cargo.toml","novaOS/kernel/Cargo.toml","novaFS","components","distribution")}
    ecrire(dest/"bilan.json",bilan)
    ecrire(dest/"inventaire.json",inventaire)
    paquet(dest,archive,inventaire)
    (dest/"SHA256SUMS").write_text("".join(f"{digest(dest/f)}  {f}\n" for f in ("bilan.json","inventaire.json","preuves.tar.gz")))


if __name__=="__main__":
    main()
