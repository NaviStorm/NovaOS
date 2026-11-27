"""Qualification de la reprise volatile A5 sur la fixture ARM de la phase 4D."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "tests"))
from gestionnaire_4d.qualifier import BASE, PORT, REPO, build, output, sha, write

CAS = dict.fromkeys(("avant-selection", "apres-selection", "reservation", "attribution",
                    "lanceur-avant", "lanceur-apres", "publication", "gestionnaire-actif",
                    "lanceur-actif", "deces-observe", "lanceur-observe", "lanceur-engage", "concurrence", "limite-numeros", "refus-certain"), "reset-court")
CAS.update({"reconciliation":"reset-iommu-differe", "quarantaine":"reset-absent",
            "journal-perdu":"reset-iommu-differe", "non-corroborable":"reset-iommu-differe",
            "double-deces":"reset-deces", "iommu-absent":"reset-iommu-absent", "endurance":"cycles-256","pression":"plein"})
GELS = {"journal-perdu","non-corroborable"}
def exiger(test, message):
    if not test:
        raise ValueError(message)

def lignes(texte, prefixe):
    return [dict(re.findall(r"(\w+)=([^ ]+)", ligne))
            for ligne in texte.splitlines() if ligne.startswith(prefixe)]

def nombre(d, cle):
    return int(d[cle])

def verifier(cas, rep):
    rapport = json.loads((rep/"campagne.json").read_text())
    texte = (rep/"serie.log").read_text()
    exiger(rapport["succes"] and rapport["elf_relu_esp"] == rapport["elf"], "oracles du port ou ELF dans l’ESP")
    exiger(rapport["volume_relu_identique"] and rapport["erreur_pair"] is None, "volume P9 ou pair")
    exiger(not any(x in texte for x in ("Exception noyau", "Panique du noyau", "Synchronous Exception")), "exception")
    if cas=="limite-numeros":
        exiger(rapport["iommu_refus"]["maps"]==0 and "E41 journal " not in texte and "E41 refus numero_epuise" in texte,"épuisement avant effet")
        return dict(scenario=cas,cycles=1,reprises_ms=[],creations=0,pic_cadres=nombre(lignes(texte,"D4 cycle avant=")[0],"pic"),nic_retenus=0,
                    trames_par_sens=0,p9=rapport["volume_p9"],journal=None,ima=rapport["elf"])
    cycles = 256 if cas=="endurance" else 1 if cas in GELS|{"refus-certain","double-deces","quarantaine","iommu-absent","pression"} else 2
    fins=lignes(texte,"E41 fin "); bilans=lignes(texte,"D4 cycle avant=")
    admissions=lignes(texte,"D4 lancement scelle "); recus=lignes(texte,"E41 recu ")
    reprises=lignes(texte,"E41 reprise "); avant=lignes(texte,"E41 reprise_selection ")
    exiger(len(fins)==len(bilans)==cycles,"bilans par cycle")
    exiger(len(admissions)==len(recus)==(0 if cas=="refus-certain" else cycles),"nombre de créations")
    exiger(len(re.findall(r"RAM phase=4d_fin_cycle .*?upile=0 .*?ipc=0 utilisateur=0 processus=0 ",texte))==cycles,"résidus de contrôle")
    for lancement,recu in zip(admissions,recus):
        exiger((lancement["generation"],lancement["pid"],lancement["incarnation"])==
               (recu["generation"],recu["pilote"],recu["incarnation"])
               and recu["creations"]=="1" and recu["rejeu"]=="sans_effet","bail, titulaire ou reçu")
    exiger(len({r["generation"] for r in admissions})==len(admissions),"bail réutilisé")
    exiger(len(re.findall("E41 quota intention_occupee=true autre_id_refuse=true cloture_reservee=true",texte))==len(recus),"quota et réserve de clôture")
    exiger(len(lignes(texte,"E41 liaisons initiales racines=[4, 4]"))==cycles+(cas=="avant-selection"),"racines des deux acteurs")
    attendues=0 if cas in GELS|{"avant-selection","iommu-absent"} else cycles
    exiger(len(reprises)==attendues and len(avant)==(cas=="avant-selection"),"point de reprise absent")
    phase={"endurance":"attribution","pression":"attribution","concurrence":"attribution","double-deces":"reconciliation"}.get(cas,cas)
    for r in reprises+avant:
        exiger(r["ancien_pid"]!=r["nouveau_pid"] and r["ancien_inc"]!=r["nouvel_inc"]
               and r["ancienne_liaison"]!=r["nouvelle_liaison"] and r["ancien_refuse"]=="true","identité ou ancienne liaison")
    for r in reprises:
        role=2 if cas.startswith("lanceur-") or cas=="refus-certain" else 1
        exiger(nombre(r,"role")==role and r["phase"]==phase,"rôle ou phase de la reprise")
        if cas=="reservation": exiger(r["mappes"]==r["epingles"]=="7" and r["pilote"]=="0","réservation partielle")
        if cas=="lanceur-avant": exiger(r["issue"]=="0" and r["pilote"]=="0","avant création")
        if cas in {"lanceur-apres","attribution","concurrence","endurance"}:
            exiger(r["issue"]=="2" and nombre(r,"pilote")>0,"création non corroborée")
        if cas=="lanceur-engage": exiger(r["issue"]=="1" and r["pilote"]=="0" and "E41 creation en_attente" in texte,"intention en vol")
        if cas=="refus-certain": exiger(r["issue"]=="3" and r["pilote"]=="0","refus avant effet")
        if cas=="quarantaine": exiger(nombre(r,"epingles").bit_count()==14,"quarantaine partielle")
        if cas in {"reconciliation","double-deces"}: exiger(nombre(r,"recu")>0 and nombre(r,"mappes").bit_count()==20,"reçu IOMMU")
    if cas in {"concurrence","double-deces"}:
        exiger(len(re.findall(r"E41 concurrence gagnant=\d+ refuse=\d+ sortie=Exit\(10\) vues_perdant=0",texte))==cycles,"deux prétendants")
    if cas in {"publication","gestionnaire-actif","lanceur-actif"}:
        exiger(texte.count("lecture_ecriture_flush_relecture=true")==cycles and len(lignes(texte,"E41 stockage sans_controle"))==cycles,"stockage pendant la vacance")
        for r in reprises: exiger(nombre(r,"publication")>0,"identité de publication")
        if cas!="publication": exiger(len(lignes(texte,"E41 RX sans_controle"))==cycles,"réception effective pendant la vacance")
    if cas in GELS:
        g=lignes(texte,"E41 gel ")
        exiger(len(g)==1 and g[0]["cause"]==cas and nombre(g[0],"epingles_proprietaire")==22,"gel et propriétaire des cadres")
    retention = 14 if cas=="quarantaine" else 22 if cas in GELS|{"iommu-absent"} else 0
    for fin,bilan in zip(fins,bilans):
        exiger(fin["compactage"]==("false" if retention else "true") and fin["gel"]==("true" if cas in GELS else "false"),"compactage du journal")
        exiger(bilan["ipc"]=="0", "pages IPC restantes")
        if not retention and cas!="refus-certain":
            exiger(bilan["avant"]==bilan["apres"] and bilan["dma"]=="16", "fuite après cycle")
        if retention: exiger(nombre(bilan,"dma")==retention+9,"allocateur DMA/quarantaine")
    latences=[1000*nombre(r,"ticks")/nombre(r,"hz") for r in reprises+avant]
    return dict(scenario=cas,cycles=cycles,reprises_ms=latences,creations=len(recus),
                pic_cadres=max(nombre(b,"pic") for b in bilans),nic_retenus=retention,
                trames_par_sens=rapport["trames_par_sens"],p9=rapport["volume_p9"],
                journal=lignes(texte,"E41 journal ")[0],ima=rapport["elf"])

def compresser(trace):
    if trace.stat().st_size<=12<<20: return
    dest=trace.with_suffix(".trace.gz")
    with trace.open("rb") as src,gzip.open(dest,"wb",compresslevel=3) as dst:
        shutil.copyfileobj(src,dst)
    with gzip.open(dest,"rb") as src:
        exiger(hashlib.file_digest(src,"sha256").hexdigest()==sha(trace),"compression de trace")
    trace.unlink()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive",type=Path);p.add_argument("scenarios",nargs="+",choices=["tous",*CAS])
    p.add_argument("--demarrages",type=int,choices=[1,2,3],default=3)
    p.add_argument("--brouillon",action="store_true",help="Mise au point sans attestation de révision")
    a=p.parse_args()
    archive=a.archive.resolve()
    if a.brouillon:
        ref="mise-au-point"
    else:
        exiger(not output("git","status","--porcelain"), "révision propre exigée pour les preuves")
        ref=output("git","rev-parse","HEAD")
    cas=list(CAS) if a.scenarios==["tous"] else a.scenarios
    echecs=[]
    for n in range(1,a.demarrages+1):
        for scenario in cas:
            rep=archive/scenario/str(n)
            try:
                image,_=build(CAS[scenario],archive,reprise=scenario,revision=ref)
                if not rep.exists():
                    files=BASE/"archives-ethernet-phase3/reproduction-8bd3f9c"
                    cmd=[sys.executable,"-B",PORT,"arm",CAS[scenario],image,rep,
                         BASE/"pilote-virtio-net",BASE/"archives-virtio-net",
                         files/"limine-enroll",files/"BOOTAA64.EFI","--limine-arm",files/"limine-a72.EFI",
                         "--source-elf",ref,"--cycle-vie","--esp-verifiee","--gestionnaire-4e1",scenario,
                         "--delai","600" if scenario=="endurance" else "60"]
                    subprocess.run(cmd,cwd=REPO,check=True)
                exiger(json.loads((rep/"campagne.json").read_text())["elf"]==sha(image),"ELF du démarrage")
                bilan=verifier(scenario,rep)
                write(rep/"reprise.json",bilan)
                trace=rep/"qemu.trace"
                if trace.exists(): compresser(trace)
                print(f"Validé {scenario} démarrage {n}",flush=True)
            except (OSError,ValueError,subprocess.CalledProcessError,KeyError,IndexError) as e:
                echecs.append((scenario,n));print(f"Échec {scenario} démarrage {n} {e}",flush=True)
    if echecs: raise SystemExit(1)

if __name__=="__main__":main()
