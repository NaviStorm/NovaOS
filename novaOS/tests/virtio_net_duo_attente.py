"""Modèle de faute Q35 distinct. Vérifications préalables et lectures après le retrait."""
import json
import os
import re
import struct
from pathlib import Path

from nvme_phase2 import application_order, processes
from nvme_phase2_oracle import chronology, inspect, io_trace, verify_transition
from virtio_net_confinement import digest, reg
from virtio_net_duo_cycles import require, resources


def verifier_preconditions(firmware, source, mode):
    root=Path(os.environ["NOVAOS_Q35_PREUVES_WAIT"]).resolve(strict=True)
    reference=Path(__file__).resolve().parents[1]/"docs/validation/virtio-net/cloture-20261126/preconditions-wait.json"
    checks=json.loads(reference.read_text())
    model=json.loads(Path(os.environ["NOVAOS_QEMU_RX_Q35"]).read_text())
    require(model.get("diff_wait")==checks["diff"] and model["sha256"]==checks["qemu"] and
            digest(Path(model["executable"]))==checks["qemu"] and digest(firmware/"OVMF_CODE.fd")==checks["ovmf"],
            "Identité du modèle expérimental et du firmware")
    require(set(checks["fermes"])=={"signature","avant-dbx","dbx","noyau-altere","sans-iommu","domaines","total-nic"},
            "Précontrôles de confinement incomplets")
    for case,files in checks["fermes"].items():
        base=root/("revue-"+case+"-01")
        require(set(files)=={"campagne.json","modele-rx.json","qemu.trace","efi.log","serie.log","fin.json","reset.json"}
                and all(digest(base/name)==h for name,h in files.items()),"Preuve modifiée pour "+case)
        r=json.loads((base/"campagne.json").read_text())
        m=json.loads((base/"modele-rx.json").read_text())
        require(r["cas"]==case and r["reussi"] and r["firmware_sha256"]==checks["ovmf"] and m["sha256"]==checks["qemu"],
                "Précontrôle fermé inapplicable")
    inputs=json.loads((source/"entrees.json").read_text())
    conf=source/"esp/limine.conf"
    require(conf.read_text().splitlines()[-1].strip()=="cmdline: net-q35-duo-"+mode and
            inputs["OVMF_CODE.fd"]==checks["ovmf"] and all(digest(p)==inputs[p.name] for p in
            (conf,source/"esp/novaos.elf",source/"esp/EFI/BOOT/BOOTX64.EFI",source/"variables.fd",source/"support.raw")),
            "Le support ou la sélection de faute diffère de la préparation signée")


def registres(vm,qt,log,fault,mode):
    initial=re.findall(rb"DUO tables root=(0x\w+) qi=(0x\w+) wait=(0x\w+) iqt=144 cookie=3 nic=true nvme=true en_attente=false",log)
    require(len(initial)==1,"Publication initiale")
    root,qi,wait=(int(x,16) for x in initial[0])
    def qtable(pa):
        qt.write(f"read {pa:#x} 4096\n".encode())
        row=qt.readline().strip()
        require(re.fullmatch(rb"OK 0x[0-9a-fA-F]{8192}",row),"Table tronquée")
        return struct.unpack("<512Q",bytes.fromhex(row[5:].decode()))
    def reg64(off):
        return reg(vm,0xfed90000+off)|(reg(vm,0xfed90004+off)<<32)
    late=fault=="tardif"; short=fault=="court"
    first=48 if mode.endswith("-disque") else 56
    order=[0,48,56,first]+([104-first] if short else [])
    tail=len(order)*48; head=tail if late or short else tail-16
    cookie=5 if short else 4 if late else 0
    require(reg64(0x20)==root and reg64(0x90)==qi and reg64(0x80)==head and reg64(0x88)==tail and
            reg(vm,wait)==cookie and reg(vm,0xfed90034)==0 and reg64(0xb8)==0,"État matériel du contrôleur commun")
    descriptors=qtable(qi)
    for i,sid in enumerate(order,1):
        did={0:0,48:1,56:3}[sid]
        cc=(sid<<32)|(did<<16)|0x31 if sid else 0x11
        tlb=(did<<16)|0xe2 if sid else 0xd2
        require(descriptors[(i-1)*6:i*6]==(cc,0,tlb,0,i<<32|0x25,wait),"Descripteurs d'invalidation ou cookie")
    require(not any(descriptors[len(order)*6:]),"Commande de plus après incertitude")
    root_page=qtable(root)
    require(root_page[0]&4095==1 and not any(root_page[1:]),"Racine différente")
    ctx={i:v for i,v in enumerate(qtable(root_page[0]&~4095)) if v}
    other=104-first
    require(set(ctx)==(set() if short else {other*2,other*2+1}) and
            (short or (ctx[other*2]&4095==1 and ctx[other*2+1]==({48:1,56:3}[other]<<8|2))),
            "Contexte résiduel divergent")
    return dict(cookie=cookie,iqh=head,iqt=tail,root=root,qi=qi,wait=wait,domaines=order,contexte=ctx)


def verifier_contention(out,mode,sector,reader,initial,old,fault):
    from virtio_net_duo_stockage import verifier
    log=(out/"serie.log").read_text()
    tr=(out/"qemu.trace").read_text()
    tokens=re.findall(r"NOVA WAIT hold mode=(\w+) cookie=4 head=11 addr=(0x[0-9a-f]+)",tr)
    cookies=re.findall(r"vtd_inv_desc_wait_sw wait invalidate status write addr (0x[0-9a-f]+) data (0x[0-9a-f]+)",tr)
    require(len(tokens)==1 and tokens[0][0]==fault and {a for a,c in cookies}=={tokens[0][1]} and
            [int(c,16) for a,c in cookies]==list(range(1,6 if fault=="court" else 5 if fault=="tardif" else 4)) and
            tr.count("NOVA WAIT release cookie=4 head=11")==int(fault!="absent"),"Défaut matériel non démontré")
    if fault=="court":
        return verifier(out,mode,sector,reader,initial,old)
    r={"secteur":sector,"faute":fault}
    processes(r,log,"contention-incertaine",1,sector)
    events=re.findall(r"DUO demande (NIC|NVMe) pid=(\d+) generation=1 doublon=refuse avant_WAIT=refuse",log)
    require(len(events)==2 and {d for d,p in events}=={"NIC","NVMe"} and len({p for d,p in events})==2,
            "Titulaire ou simultanéité des demandes")
    msg="DUO contention demandes=2 premier_disque="+("true" if mode.endswith("-disque") else "false")+" dma=40"
    require(log.count(msg)==1 and all(log.index("DUO demande "+d)<log.index(msg) for d,p in events),"Deux demandes réellement en attente")
    disk, nic=map(int,[dict(events)["NVMe"],dict(events)["NIC"]])
    require(disk==r["processus"]["pilote"] and f"DUO réseau instance={nic} génération=1 drainage=Ok(())" in log,
            "Drainage physique des deux pilotes")
    require("NET L2 total TX=48 RX=48 index=48 rearmements=2" in log and
            log.count("DUO contention WAIT incertain cookie=0 iqh=176 iqt=192 dma=40")==1 and
            log.count(f"DUO contention deux_retraits_refuses PID={disk}/{nic} nouvelles_demandes=refusees cookie="+
                      ("4 iqh=192" if fault=="tardif" else "0 iqh=176")+" iqt=192 dma=40")==1 and
            "publication=false" not in log and "DUO restitution " not in log and
            "NVME echec retrait non confirme" in log,"Décision native, quarantaine ou réponse après expiration")
    expected=[(str(disk if sid==48 else nic),str(sid),str(cookie)) for sid,cookie in ((48,2),(56,3))]
    actual=[(pid,sid,c) for sid,pid,c in re.findall(r"DUO WAIT sid=(\d+) did=\d+ instance=(\d+) generation=1 publication=true cookie=(\d+)",log)]
    require(actual==expected,"Reçu inventé ou manquant")
    commands=[int(x,16) for x in re.findall(r"^vtd_reg_write_gcmd .* value (0x[0-9a-f]+)$",tr,re.M)]
    require(commands==[0x40000000,0x04000000,0x84000000] and
            not re.search(r"^vtd_(dmar_fault|ir_enable enable 1|reg_ir_root)",tr,re.M),"État matériel commun")
    enabled={48:False,56:False}; edges={48:[],56:[]}; nwait=0
    for n,row in enumerate(tr.splitlines()):
        if row.startswith("vtd_inv_desc_wait_sw "): nwait+=1
        if row.startswith("pci_cfg_write "):
            m=re.fullmatch(r"pci_cfg_write \S+ 00:([0-9a-f]+)\.(\d) @(0x[0-9a-f]+) <- (0x[0-9a-f]+)",row)
            require(m,"Écriture PCI inconnue")
            slot,fn,off,v=(int(x,16) for x in m.groups()); sid=slot*8+fn
            require(sid in (0,48,56,248,251) and off>=4,"Topologie Q35")
            if off==4:
                on=bool(v&4)
                require(not on or (sid in enabled and nwait>=(2 if sid==48 else 3) and v in ((0x406,) if sid==48 else (6,0x406))),
                        "Armement hors de l'attribution")
                if sid in enabled and on!=enabled[sid]:
                    edges[sid].append(n); enabled[sid]=on
    require(all(len(e)==2 for e in edges.values()) and not any(enabled.values()),"Désarmement des deux maîtres")
    ops=io_trace(out/"qemu.trace",sector)
    require(all(op.get("cqe")==0 for op in ops),"E/S du stockage sans CQE")
    final=inspect(reader,out/"namespace.raw",sector)
    verify_transition(old,final,{"nouveau"})
    r.update(p9=final,ordre=application_order(chronology(ops,old,sector),log),demandes=events,bme=edges,
             apres=resources(log,"nvme_apres_retrait"),avant=resources(log,"nvme_session"),pendant=resources(log,"net_files_pleines"))
    roots=[(int(p),int(n)) for p,n in re.findall(r"DUO IPC pid=(\d+) racines=(\d+)",log)]
    require(len(roots)==5 and max(n for p,n in roots)<=4 and r["apres"]["dma"]==40 and
            all(r["apres"][key]==0 for key in ("ipc","processus","utilisateur","upile","meta_actifs")),"Titularité ou restitution finale")
    owner="NVMe" if mode.endswith("-disque") else "NIC"
    require(f"DUO issue retrait {owner}=Quarantine" in log,"Statut natif du retrait")
    r["deux_demandes"]=resources(log,"duo_deux_demandes")
    r["quarantaine"]=resources(log,"duo_wait_incertain")
    require(r["deux_demandes"]["dma"]==r["quarantaine"]["dma"]==40,"Cadres durant le WAIT")
    r["racines"]=roots
    return r
