"""Relire les preuves locales d'une matrice NVMe complète et publier un JSON compact."""
import argparse
import json
from pathlib import Path

from nvme_phase2 import (ACK, CUTS, EIO, MODES, application_order, chronology, eio_proof,
                         incident, processes, select_cases)
from nvme_phase2_oracle import inspect, io_trace, unconfirmed_write, verify_transition
from materiel_pilote import sha256
from vtd_edu import require


def check_vm(folder, vm, root):
    sector = vm["secteur"]
    mode = vm["mode"]
    require(sha256(folder/"serie.log")==vm["sha256_serie"] and
            sha256(folder/"trace.log")==vm["sha256_trace"], "Journaux bruts modifiés")
    iso = root/f"nvme-rw-{'update' if mode in EIO else mode}.iso"
    require(sha256(iso)==vm["sha256_iso"], "ISO différente de l'essai")
    log = (folder/"serie.log").read_text(errors="replace")
    ops = io_trace(folder/"trace.log",sector)
    processes(vm,log,mode,vm["vcpus"],sector)
    require((len(ops), sum(o["op"]=="WRITE" for o in ops), sum(o["op"]=="FLUSH" for o in ops))
            == (vm["commandes"],vm["write"],vm["flush"]), "Compteurs NVMe incohérents")
    return log,ops


def compact(root, reader):
    raw_report=root/"bilan.json"
    report=json.loads(raw_report.read_bytes())
    require(sha256(root/"novaos.elf")==report["sha256_elf"] and sha256(reader)==report["sha256_lecteur"],
            "Empreinte des exécutables")
    seeds={}
    media={}
    for initial in report["initialisations"]:
        vm=initial["vm"]
        s=vm["secteur"]
        d=root/f"initial-{s}"
        log,_=check_vm(d/"creation",vm,root)
        require("NVME P9 creation confirmee" in log and sha256(d/"initial.raw")==vm["sha256_raw_apres"],
                "Initialisation incomplète ou fichier de base modifié")
        seeds[s]=inspect(reader,d/"initial.raw",s)
        media[s]={"sha256_raw":vm["sha256_raw_apres"],"generation":seeds[s]["generation"],
                  "sha256_ancien":seeds[s]["sha256_fichier"],"garde_sha256":seeds[s]["sha256_garde"],
                  "write":vm["write"],"flush":vm["flush"],"sha256_serie":vm["sha256_serie"],
                  "sha256_trace":vm["sha256_trace"]}
    require(set(seeds)=={512,4096}, "Une géométrie n'a pas été créée")
    rows=[]
    for c in report["cellules"]:
        name, n, s = c["point"], c["vcpus"], c["secteur"]
        d=root/c.get("dossier",f"{name}-{n}-{s}")
        require(d.resolve().parent==root.resolve(), "Répertoire de preuve hors campagne")
        vm, cold = c["operation"],c["remontage"]
        log, ops=check_vm(d/"operation",vm,root)
        check, _=check_vm(d/"remontage",cold,root)
        latest=inspect(reader,d/"disque.raw",s)
        require(vm["sha256_raw_avant"]==media[s]["sha256_raw"] and
                sha256(d/"disque.raw")==vm["sha256_raw_apres"]==cold["sha256_raw_avant"]==cold["sha256_raw_apres"]
                and "NVME P9 oracle "+latest["etat"] in check and not cold["write"] and not cold["flush"],
                "Les données et les deux résultats de montage doivent correspondre")
        compact_row={"cas":name,"vcpus":n,"secteur":s,"generation":latest["generation"],
                     "etat":latest["etat"],"fichier_sha256":latest["sha256_fichier"],
                     "manifeste_sha256":latest["ancres"][latest["slot"]]["sha256"],
                     "raw_sha256":cold["sha256_raw_apres"],"ack":ACK in log,
                     "opcode":{"read":len(ops)-vm["write"]-vm["flush"],"write":vm["write"],"flush":vm["flush"]},
                     "temps_hote_s":vm["secondes_ecoulees_hote"],
                     "temps_bloc_hote_ns":{k:v for k,v in vm["compteurs_qemu"].items() if k.endswith("_ns")},
                     "serie_sha256":[vm["sha256_serie"],cold["sha256_serie"]],
                     "trace_sha256":[vm["sha256_trace"],cold["sha256_trace"]]}
        if name == "cycle" or name in CUTS:
            tx = chronology(ops,seeds[s],s,name if name in CUTS else None)
            compact_row["ordre"]=tx
            if name=="cycle":
                compact_row["acquittement"]=application_order(tx,log)
                allow=("nouveau",)
            else:
                require(ACK not in log and vm["arret"]=="SIGKILL" and vm["breakpoint_confirme"]==CUTS[name],
                        "Acquittement ou coupure incohérents")
                allow=(("ancien","nouveau") if name in ("during-publish","during-f2")
                       else ("nouveau",) if name=="after-f2" else ("ancien",))
        elif name in EIO:
            compact_row["signalement"]=eio_proof(name,vm,ops,log)
            allow=("ancien","nouveau") if name=="eio-f2" else ("ancien",)
        else:
            allow=incident(name,vm,ops,log)
            if name == "dma-fault":
                impact=unconfirmed_write(root/f"initial-{s}"/"initial.raw",d/"disque.raw",ops,s)
                require(impact == c["impact_write_sans_confirmation"], "Impact sur la RAW non reproductible")
                compact_row["impact_write_sans_confirmation"]=impact
        verify_transition(seeds[s],latest,allow)
        domain=vm["domaine"]
        if domain is not None:
            compact_row["retrait"]={k:domain[k] for k in ("qi_et_wait","fsts","iqh","iqt","cookie_lu_hote",
                                     "retrait_confirme","faute_source","source_témoin_sha256") if k in domain}
            if "cadres_clients_mappes" in domain:
                compact_row["retrait"]["droits_dma_residuels"]=[e["droits"] for e in domain["cadres_clients_mappes"]]
        if vm.get("cqe_tardive_apres_bilan"):
            compact_row["cqe_tardive"]=vm["cqe_tardive_apres_bilan"]
        if vm["migration"] or vm["revocation"]:
            compact_row["migration"]=vm["migration"]
            compact_row["revocation"]=vm["revocation"]
        rows.append(compact_row)
    expected={(m,n,s) for m in MODES for n,s in select_cases(m,None,None)}
    got={(r["cas"],r["vcpus"],r["secteur"]) for r in rows}
    require(got==expected and len(rows)==len(expected), "Matrice incomplète ou doublon de cellule")
    return {k:report[k] for k in ("sha256_elf","sha256_lecteur","firmware","qemu","geometrie")} | {
        "machine":"pc-q35-11.1, TCG, cpu max, 256 Mio, RAW local, cache=writeback, no-flush=false",
        "coupe":"SIGKILL du seul processus QEMU, Mac alimenté. La reprise utilise une nouvelle VM et un disque présenté en lecture seule.",
        "sha256_bilan_complet":sha256(raw_report),"creation":media,"cellules":rows}


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("dossier",type=Path)
    p.add_argument("--lecteur",required=True,type=Path)
    p.add_argument("--rapport",required=True,type=Path,help="Nouveau fichier JSON")
    args=p.parse_args()
    result=compact(args.dossier,args.lecteur.resolve(strict=True))
    with args.rapport.open("x") as out:
        json.dump(result,out,ensure_ascii=False,indent=2)
        out.write("\n")
