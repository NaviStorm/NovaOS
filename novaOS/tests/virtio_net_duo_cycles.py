"""Quatre générations NIC/NVMe sous une seule racine VT-d dans le même invité."""
import re
from nvme_phase2 import processes, application_order
from nvme_phase2_oracle import chronology, inspect, io_trace
from virtio_net_confinement import digest


def require(value, message):
    if not value:
        raise ValueError(message)


def resources(log, phase):
    rows = re.findall(r"^RAM phase="+phase+r" ([^\n]+)", log, re.M)
    require(len(rows)==1, "Relevé manquant ou dupliqué pour "+phase)
    return {k:int(v) for k,v in re.findall(r"(\w+)=(\d+)", rows[0])}


def verifier_cycles(out, sector, reader, initial, old):
    log=(out/"serie.log").read_text()
    trace=(out/"qemu.trace").read_text().splitlines()
    image=out/"namespace.raw"
    final=inspect(reader,image,sector)
    require(final["etat"]=="nouveau" and final["generation"]==old["generation"]+4,
            "Quatre publications durables du contenu témoin requises")
    operations=io_trace(out/"qemu.trace",sector,epochs=4)
    ids=set()
    reports=[]
    storage_slot=old["slot"]
    ids_by_cycle=[]
    for g in range(1,5):
        start=f"DUO debut cycle={g}\n"
        end=f"DUO fin cycle={g}\n"
        require(log.count(start)==log.count(end)==1,"Marqueurs de cycle")
        part=log.split(start)[1].split(end)[0]
        proc={}
        processes(proc,part,"update",1,sector)
        disk=proc["processus"]["pilote"]
        who=re.findall(r"NET Q35 pilote pid=(\d+) mode=3 racines=1 generation=(\d+)",part)
        require(len(who)==1 and int(who[0][1])==g,"Instance réseau")
        nic=int(who[0][0])
        roles=set(map(int,proc["processus"]["retours"]))|{nic}
        require(len(roles)==5 and not roles&ids,"Réutilisation de PID")
        ids.update(roles)
        ids_by_cycle.append((disk,nic))
        roots=[(int(p),int(r)) for p,r in re.findall(r"DUO IPC pid=(\d+) racines=(\d+)",part)]
        require(len(roots)==5 and set(p for p,r in roots)==roles and max(r for p,r in roots)<=4,"Racines IPC")
        require(f"DUO réseau instance={nic} génération={g} drainage=Ok(())" in part and
                "NET L2 total TX=48 RX=48 index=48 rearmements=2" in part and
                "NET Q35 L2 GSI=23 ISR=0x101" in part and "NET L2 pilote fin=Exit(0) racines=0" in part,
                "Drainage, interruptions ou fin du pilote réseau")
        require(f"NVME retrait epoque={g} controleur=confirme BME=0 contexte=0 IOTLB=drain WAIT=confirme pages_restituees=6" in part,
                "Retrait du disque")
        ios=[o for o in operations if o["epoque"]==g]
        require(ios and all(o["cqe"]==0 for o in ios),"CQE de l'incarnation")
        tx=chronology(ios,{"slot":storage_slot},sector)
        order=application_order(tx,part)
        storage_slot=1-storage_slot
        before=resources(part,"duo_avant_cycle")
        peak=resources(part,"net_files_pleines")
        after=resources(part,"nvme_apres_retrait")
        require(before["dma"]==after["dma"]==8 and peak["dma"]==40 and
                all(after[k]==0 for k in ("ipc","processus","utilisateur","upile","meta_actifs")),
                "Comptes des deux baux après retrait")
        reports.append(dict(generation=g,processus=proc["processus"],nic=nic,racines=roots,
                            ordre=order,avant=before,files_pleines=peak,apres=after))
    stable=("resident","retenu","libres","tables","kpile","upile","heap","ipc","meta_octets","tcb_octets","cache","dma")
    for row in reports[1:]:
        previous=reports[row["generation"]-2]["apres"]
        require(all(row["avant"][k]==previous[k] for k in stable),"Dérive entre les cycles")
        require(all(row["apres"][k]==reports[0]["apres"][k] for k in stable),"Croissance après restitution")

    # La première invalidation publie la racine vide. Chaque bail renouvelle
    # ensuite deux publications et deux retraits individuels dans cette racine.
    sid_order=[0]+[48,56,56,48]*4
    waits=[]; descriptors=[]; address=None; changes=[]; gcmd=[]; enabled={48:False,56:False}
    for n,line in enumerate(trace,1):
        if line.startswith("vtd_reg_write_gcmd "):
            gcmd.append(int(line.split(" value ")[1],16))
        if line.startswith(("vtd_dmar_fault","vtd_ir_enable enable 1","vtd_reg_ir_root")):
            raise ValueError("Faute VT-d ou remappage d'interruption incompatible")
        if line.startswith("vtd_inv_desc invalidate desc type "):
            descriptors.append(line.split(" type ",1)[1])
        if line.startswith("vtd_inv_desc_wait_sw "):
            fields=line.split()
            addr,cookie=int(fields[-3],16),int(fields[-1],16)
            require(cookie==len(waits)+1 and cookie<=len(sid_order),"Cookie QI")
            sid=sid_order[cookie-1]
            did={0:0,48:1,56:3}[sid]
            cc=(sid<<32)|(did<<16)|0x31 if sid else 0x11
            tlb=(did<<16)|0xe2 if sid else 0xd2
            require(descriptors==[f"context-cache high 0x0 low {cc:#x}",f"iotlb high 0x0 low {tlb:#x}",
                                  f"wait high {addr:#x} low {cookie<<32|0x25:#x}"],"Portée des invalidations")
            require(address in (None,addr),"Page de reçu différente")
            descriptors.clear(); address=addr; waits.append(n)
        if line.startswith("pci_cfg_write "):
            m=re.fullmatch(r"pci_cfg_write \S+ 00:([0-9a-f]+)\.(\d) @(0x[0-9a-f]+) <- (0x[0-9a-f]+)",line)
            require(m,"Écriture PCI non décodée")
            slot,fn,off,value=(int(s,16) for s in m.groups())
            sid=slot*8+fn
            require(sid in (0,48,56,248,251) and off>=4,"Topologie PCI")
            if off==4:
                on=bool(value&4)
                require(not on or (sid in enabled and value in ((0x406,) if sid==48 else (6,0x406))
                                   and gcmd[-1:]==[0x84000000]),"Armement avant traduction")
                if sid in enabled and enabled[sid]!=on:
                    changes.append((sid,n,on)); enabled[sid]=on
    require(len(waits)==17 and not descriptors and len(changes)==16 and
            gcmd==[0x40000000,0x04000000,0x84000000],"Nombre de transactions QI/PCI")
    receipts=[]
    for g,(disk,nic) in enumerate(ids_by_cycle,1):
        offset=4*(g-1)+1
        for sid,pid,a,b in ((48,disk,0,3),(56,nic,1,2)):
            edges=[(n,on) for s,n,on in changes if s==sid][2*(g-1):2*g]
            require(len(edges)==2 and edges[0][1] and not edges[1][1] and
                    waits[offset+a]<edges[0][0]<edges[1][0]<waits[offset+b],"BME du bail hors fenêtre")
        for j,(sid,pid,pub) in enumerate(((48,disk,True),(56,nic,True),(56,nic,False),(48,disk,False))):
            cookie=offset+j+1
            receipts.append(f"DUO WAIT sid={sid} did={1 if sid==48 else 3} instance={pid} generation={g} publication={str(pub).lower()} cookie={cookie} iqt={cookie*48}")
    require(re.findall(r"^DUO WAIT .+$",log,re.M)==receipts,"Acquittements individuels des deux pilotes")
    return dict(cycles=reports,attentes=waits,changements_pci=changes,p9=final,
                sha256_initial=initial,sha256_final=digest(image),sha256_lecteur=digest(reader))
