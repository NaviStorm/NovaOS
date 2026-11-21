"""Trois démarrages, même autorité durable et mêmes détenteurs hors serveur."""
from contextlib import ExitStack,closing
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import sqlite3
import struct
from recuperation_qemu import run,fixture,sha,SERVER,DOMAIN
from rpc_recuperation import Dispatcher
from recuperation import Refus,digest
from reprise import open_database

def qmp(channel,operation):
    channel.write((json.dumps({"execute":operation})+"\n").encode());channel.flush()
    while True:
        row=json.loads(channel.readline())
        if "error" in row:raise Refus("QMP: "+str(row["error"]))
        if "return" in row:return row["return"]

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"]);p.add_argument("sector",type=int,choices=[512,4096])
    for name in ("iso","registre","genese","cles","decision","secrets","sortie","detenteur","capture","triplet","ancienne-autorite"):
        p.add_argument("--"+name,type=lambda x:Path(x).resolve(),required=True)
    p.add_argument("--mesures-finales",action="store_true")
    p.add_argument("--resilience",action="store_true",help="pertes des deux reçus, rejeu historique et autorité hors ligne")
    a=p.parse_args();a.mode="nominal";a.vfs=a.vue=a.activation=True;a.mutation=False;a.lecture_privee="autorisee"
    a.sortie.mkdir(parents=True,exist_ok=False);a.secrets.mkdir(parents=True,exist_ok=False,mode=0o700)
    a.legacy_disk=a.ancienne_autorite
    root=a.sortie;report={"profils":[a.arch,a.sector],"succes":False,
        "limites":"QEMU, autorité et deux détenteurs hors périmètre de restauration ; aucun matériel de production"}
    def scope(authority):
        with closing(open_database(authority.path)) as db:
            live,epoch,generation,head,_,_=authority._scope(db,SERVER,DOMAIN)
            return {"incarnation":live.hex(),"epoque":epoch,"generation":generation,"empreinte":head.hex()}
    with ExitStack() as stack:
        original,holders,administrator=fixture(a,stack)
        authority=original.authority
        first=copy.copy(a);first.sortie=root/"avant-retrait";first.mutation=True
        if a.resilience:first.mode="publication-capture-perdues"
        def snapshot(disk,out,monitor,body,stage):
            if stage=="publication":
                if not (root/"instantane.p9").exists():raise Refus("instantané avant préparation manquant")
                return
            if (root/"instantane.p9").exists():return
            serial=(out/"invite.serial").read_bytes()
            if b"NARV Alice nouvelle lecture L " not in serial:return
            expected=bytes(8)+(8).to_bytes(8,"little")+b"ALICE-P\n"
            if b"NARV Alice P "+expected.hex().encode() not in serial:raise Refus("lecture avant instantané absente")
            # Première consultation fraîche après la dernière lecture. Elle
            # précède la planification et toute écriture de la mutation dans R.
            # Le registre attend encore cette réponse ; aucune préparation P9.
            with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as connection:
                connection.connect(str(monitor))
                with connection.makefile("rwb",buffering=0) as channel:
                    json.loads(channel.readline());qmp(channel,"qmp_capabilities")
                    qmp(channel,"stop")
                    if qmp(channel,"query-status")["status"]!="paused":raise Refus("invité non arrêté")
                    with disk.open("rb") as f:os.fsync(f.fileno())
                    shutil.copyfile(disk,root/"instantane.p9")
                    shutil.copyfile(out/"ancienne-autorite-locale.raw",root/"instantane-ancienne-autorite.raw")
                    report["avant_retrait"]=scope(authority)
                    report["instantane_sha256"]=sha(root/"instantane.p9")
                    report["ancienne_autorite_sha256"]=sha(root/"instantane-ancienne-autorite.raw")
                    report["mode_instantane"]="invité suspendu pendant consultation fraîche, avant préparation et publication du retrait"
                    qmp(channel,"cont")
        rc=run(first,(original,holders,administrator),snapshot)
        report["avant_retrait_rc"]=rc
        if rc:raise Refus("parcours de lecture/retrait/capture en échec")
        if not json.loads((first.sortie/"resultat.json").read_text()).get("capture_invitee"):
            raise Refus("capture invitée non démontrée")
        report["apres_retrait"]=scope(authority)
        if report["apres_retrait"]["generation"]!=report["avant_retrait"]["generation"]+1:
            raise Refus("révocation non publiée")
        files=list((first.sortie/"captures-invite").glob("*/manifeste.bin"))
        if len(files)!=1:raise Refus("capture absente ou ambiguë")
        manifest=files[0].read_bytes()
        chunks=[x.read_bytes() for x in sorted(files[0].parent.glob("*.chiffre"))]
        manifests=dict(original.manifests);manifests[digest(manifest)]=manifest
        ciphertexts=dict(original.ciphertexts);ciphertexts[digest(manifest)]=chunks
        def dispatcher(approve):
            return Dispatcher(authority,original.enrollment,SERVER,DOMAIN,original.peer_key,original.measurement,
                manifests,ciphertexts,approve,original.deliver_independently,original.retrieve_independently)
        def deny(*args):raise Refus("reprise non approuvée par administrateur indépendant")
        quarantine=copy.copy(a);quarantine.sortie=root/"restauration-sans-approbation"
        quarantine.legacy_disk=root/"instantane-ancienne-autorite.raw"
        quarantine.genese=root/"instantane.p9";quarantine.quarantaine=True;quarantine.lecture_privee="refusee"
        report["sans_approbation_rc"]=run(quarantine,(dispatcher(deny),holders,administrator))
        if report["sans_approbation_rc"]:raise Refus("refus VFS de l'état restauré non démontré")
        if scope(authority)!=report["apres_retrait"]:raise Refus("autorité restaurée ou mutée")
        if a.resilience:
            # L'ancien reçu de tête reste un document historique, jamais une
            # réponse fraîche ni une autorisation. Deux invités réels séparés.
            for label,extra in (("rejeu-tete-historique",{"tete_rejouee":first.sortie/"tete-historique.bin"}),
                                ("autorite-hors-ligne",{"autorite_hors_ligne":True})):
                negative=copy.copy(quarantine);negative.sortie=root/label
                for k,v in extra.items():setattr(negative,k,v)
                report[label+"_rc"]=run(negative,(dispatcher(deny),holders,administrator))
                if report[label+"_rc"]:raise Refus("refus VFS non démontré : "+label)
                if scope(authority)!=report["apres_retrait"]:raise Refus("autorité altérée")
        # Mandat administratif explicite hors invité, autorise une seule incarnation.
        keys=[holders[0].keys[0],holders[1].keys[0],holders[0].keys[1],holders[1].keys[1],holders[0].keys[2],holders[1].keys[2]]
        administrator.call("mandate",SERVER,DOMAIN,original.peer_key,original.measurement,digest(manifest),
            int.from_bytes(manifest[78:86],"big"),manifest[270:302],keys)
        recovered=copy.copy(a);recovered.sortie=root/"reprise-approuvee";recovered.genese=root/"instantane.p9"
        recovered.lecture_privee="refusee"
        recovered.legacy_disk=root/"instantane-ancienne-autorite.raw"
        report["reprise_rc"]=run(recovered,(dispatcher(original.approve_independently),holders,administrator))
        report["apres_reprise"]=scope(authority)
        report["succes"]=(report["reprise_rc"]==0 and report["apres_reprise"]["generation"]==report["apres_retrait"]["generation"] and
            report["apres_reprise"]["empreinte"]==report["apres_retrait"]["empreinte"] and
            report["apres_reprise"]["incarnation"]!=report["apres_retrait"]["incarnation"] and
            sha(quarantine.genese)==report["instantane_sha256"])
    (root/"bilan.json").write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n")
    return 0 if report["succes"] else 1
if __name__=="__main__":raise SystemExit(main())
