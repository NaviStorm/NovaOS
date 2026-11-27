"""Vérifier les deux titulaires, le journal et le volume sur le contrôleur Q35 partagé."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

from qualifier import BASE, NOVA, output, sha, verifier_lancements, verifier_remplacements, write

BANC = NOVA/"tests/ethernet_v1"


def verifier(dest, cas, scenario, *, enregistrer=True):
    c = json.loads((dest/"campagne.json").read_text())
    serie = (dest/"serie.log").read_text()
    if not c["succes"] or not c["stockage"]["p9"] or sha(dest/"namespace.raw")!=c["p9_final"]:
        raise ValueError("Volume ou contrôle indépendant")
    if cas=="duo-cycles":
        doc = controle_cycles(c, serie)
        if enregistrer:
            write(dest/"reprise.json", doc)
        return doc
    (boot,) = re.findall(r"E41 demarrage boot=([0-9a-f]{32})",serie)
    verifier_lancements(serie, 1, boot)
    (recu,) = re.findall(r"E41 recu generation=1 intention=(\d+) pilote=(\d+) incarnation=(\d+) creations=1 rejeu=sans_effet",serie)
    (proprietaire,) = re.findall(r"DUO WAIT sid=56 did=3 instance=(\d+) generation=1",serie)[:1]
    if proprietaire!=recu[1] or len(re.findall("D4 lancement scelle",serie))!=1:
        raise ValueError("Titulaire du bail et pilote lancé")
    m = re.search(r"E41 fin generation=1 creations=1 epingles=(\d+) mappes=(\d+) recu=(\d+) gel=(\w+) compactage=(\w+)",serie)
    if not m:
        raise ValueError("Clôture du journal du pilote")
    manque = cas=="duo-absent"
    if manque:
        if m[5]!="false" or m[1]!="4194303" or m[3]!="0":
            raise ValueError("Protection des cadres sans livraison")
    if not manque and (m.groups()[:2]!=("0","0") or int(m[3])<=0 or m.groups()[3:]!=("false","true")):
        raise ValueError(("Journal de restitution",m.groups()))
    remplacements=[v for v in serie.splitlines() if v.startswith("E41 reprise phase=")]
    if len(remplacements)!=int(scenario!="nominal") or any("ancien_refuse=true" not in v for v in remplacements):
        raise ValueError("Changement d’incarnation")
    if scenario!="nominal" and f"phase={scenario} " not in remplacements[0]:
        raise ValueError("Point de remplacement")
    verifier_remplacements(scenario, serie, remplacements)
    if not manque and cas!="port" and not c.get("garde",{}).get("cqe_retrouvee"):
        raise ValueError("Refus de réadmission sur commande en vol")
    if cas in ("duo-differe","duo-absent") and "nvme_actif" not in c:
        raise ValueError("Instantané du domaine indépendant absent")
    doc = dict(boot=boot, creation=recu, titulaire=proprietaire, fin=m.groups(),
               remplacements=remplacements, p9=c["p9_final"])
    if scenario in {"gestionnaire-actif", "lanceur-actif"}:
        doc["cqe_sans_controle"] = verifier_vacance_nvme(dest, serie, scenario)
    if enregistrer:
        write(dest/"reprise.json", doc)
    return doc


def verifier_vacance_nvme(dest, serie, scenario):
    role = 2 if scenario == "lanceur-actif" else 1
    bornes = (f"E41 vacance role={role} ", "F4 observation CQE ouverte",
              "F4 observation CQE fermee masque=7", f"F4 publication sans_controle role={role} ",
              "E41 RX sans_controle ", f"E41 reprise phase={scenario} ")
    if any(serie.count(marque) != 1 for marque in bornes):
        raise ValueError("Vacance, témoin NVMe ou reprise absent")
    rangs = [serie.index(marque) for marque in bornes]
    if rangs != sorted(rangs):
        raise ValueError("Ordre du retrait et de la reprise du rôle")
    cqes = re.findall(r"^E3D CQE op=(READ|WRITE|FLUSH) cid=(\d+)", serie[rangs[1]:rangs[2]], re.M)
    trace = (dest/"qemu.trace").read_text()
    opcodes = {"READ": "2", "WRITE": "1", "FLUSH": "0"}
    if {op for op, _ in cqes} != set(opcodes):
        raise ValueError("La vacance n’a pas couvert les trois opérations de stockage")
    for op, cid in cqes:
        if not re.search(rf"^pci_nvme_io_cmd cid {cid} nsid 0x1 sqid 1 opc 0x{opcodes[op]} ", trace, re.M):
            raise ValueError(("Commande absente de la trace QEMU", op, cid))
    return cqes


def controle_cycles(c, serie):
    # Un nouveau bail de stockage doit précéder chacun des 256 titulaires NIC.
    lots = c["stockage"]["cycles"]
    if len(lots)!=256 or c["trames_par_sens"]!=256*144 or c["stockage"]["cadres_nic_retenus"]!=0:
        raise ValueError("Endurance Q35 double")
    boots=set(re.findall(r"E41 demarrage boot=([0-9a-f]{32})",serie))
    recus=re.findall(r"E41 recu generation=(\d+) intention=(\d+) pilote=(\d+) incarnation=(\d+) creations=1 rejeu=sans_effet",serie)
    fins=re.findall(r"E41 fin generation=(\d+) creations=1 epingles=0 mappes=0 recu=(\d+) gel=false compactage=true",serie)
    baux=re.findall(r"DUO WAIT sid=(48|56) did=\d+ instance=(\d+) generation=(\d+)",serie)
    if len(boots)!=1 or len(recus)!=256 or len(fins)!=256 or len(baux)!=1024:
        raise ValueError("Reçus ou titulaires incomplets")
    verifier_lancements(serie, 256, next(iter(boots)))
    for i, (ligne, recu, fin) in enumerate(zip(lots,recus,fins),1):
        quatre=baux[(i-1)*4:i*4]
        if not (ligne["generation"]==i and int(recu[0])==i and int(fin[0])==i and int(fin[1])==4*i
                and [sid for sid,_,_ in quatre]==["48","56","56","48"]
                and all(int(g)==i for _,_,g in quatre) and quatre[0][1]==quatre[3][1]
                and quatre[1][1]==quatre[2][1]==recu[2] and quatre[0][1]!=recu[2]):
            raise ValueError(("Baux de la génération",i,quatre,fin))
    if len({p for _,_,p,_ in recus})!=256 or len({inc for _,_,_,inc in recus})!=256:
        raise ValueError("Réutilisation de titulaire")
    return dict(boot=boots.pop(),cycles=256,recus=recus,fins=fins,p9=c["p9_final"])


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive",type=lambda x:Path(x).resolve())
    p.add_argument("scenario",choices=("nominal", "deces-observe", "lanceur-observe", "reconciliation", "gestionnaire-actif", "lanceur-actif"))
    p.add_argument("cas",nargs="+",choices=("port","duo-cycles","duo-retrait","duo-differe","duo-absent"))
    p.add_argument("--boots",type=int,choices=(1,2,3),default=3)
    p.add_argument("--revision",default="HEAD")
    p.add_argument("--compiler",action="store_true")
    a=p.parse_args()
    revision=output("git","rev-parse","--verify",a.revision+"^{commit}")
    images=a.archive/("duo-"+a.scenario)
    if not images.exists():
        subprocess.run([sys.executable,"-B",BANC/"port_construire_q35.py",images,"--revision",revision,
                        "--phase3d","--phase4f",a.scenario,"--cas",*a.cas],check=True)
    doc=json.loads((images/"sources.json").read_text())
    if doc["port"]!=revision or doc["gestionnaire_4f"]!=a.scenario or any(
            doc["elf"].get(cas)!=sha(images/(cas+".elf")) for cas in a.cas):
        raise ValueError("Images du profil double divergentes")
    if a.compiler: return
    for cas in a.cas:
        epochs=set()
        for n in range(1,a.boots+1):
            dest=a.archive/"double"/a.scenario/cas/str(n)
            if not (dest/"campagne.json").exists():
                cmd=[sys.executable,"-B",BANC/"port_q35_stockage.py",cas,images/(cas+".elf"),dest,
                     BASE/"pilote-virtio-net",BASE/"archives-virtio-net",
                     BASE/"archives-ethernet-phase3/reproduction-8bd3f9c/limine-enroll",
                     BASE/"pilote-virtio-net/novaFS/target/aarch64-apple-darwin/release/novafs"]
                if cas in ("duo-differe","duo-absent"): cmd.append("--instantane")
                subprocess.run(cmd,check=True)
            verifier(dest,cas,a.scenario)
            epochs.add(json.loads((dest/"reprise.json").read_text())["boot"])
        if len(epochs)!=a.boots: raise ValueError("Époques de démarrage répétées")


if __name__=="__main__":
    main()
