"""Vérifie le stockage Q35 PIO, seul ou avec progression Ethernet matérielle."""
import ast
import math
import os
import re
from nvme_phase2 import application_order, processes
from nvme_phase2_oracle import chronology, inspect, io_trace, verify_transition
from virtio_net_confinement import digest


def verifier_trafic(trace, serial, mode, driver):
    peer = re.findall(r"NET Q35 pilote pid=(\d+) mode=3 racines=1 generation=1",serial)
    if len(peer)!=1 or int(peer[0])==driver or "NET L2 pilote fin=Exit(0) racines=0" not in serial:
        raise ValueError("Identité des pilotes ou restitution du canal")
    netpid=int(peer[0])
    series = {"rearmement": (48, 48, 2), "endurance": (100_000, 34_464, 6_249), "stockage": (1_000_000, 16_960, 62_499)}
    count, index, rearms = series[os.environ["NOVAOS_NET_SERIE"]]
    if (f"DUO réseau instance={netpid} génération=1 drainage=Ok(())" not in serial or
        "NET Q35 L2 GSI=23 ISR=0x101" not in serial or
        f"NET L2 total TX={count} RX={count} index={index} rearmements={rearms}" not in serial):
        raise ValueError("Flux Ethernet, IRQ ou drainage incomplet")
    order=[0,48,56] + ([48,56] if mode.endswith("-disque") else [56,48])
    waits=[]; writes=[]; submits=[]; fills=[]; commands=[]; desc=[]
    enabled={48:False,56:False}
    for n,line in enumerate(trace.splitlines(),1):
        if line.startswith("vtd_inv_desc invalidate desc type "):
            desc.append(line.split(" type ",1)[1])
        if line.startswith("vtd_inv_desc_wait_sw"):
            m=re.fullmatch(r"vtd_inv_desc_wait_sw wait invalidate status write addr (0x[0-9a-f]+) data (0x[0-9a-f]+)",line)
            if not m: raise ValueError("WAIT inconnu")
            addr,cookie=int(m[1],16),int(m[2],16)
            if cookie!=len(waits)+1 or cookie>len(order): raise ValueError("Séquence QI")
            sid=order[cookie-1]; did={0:0,48:1,56:3}[sid]
            cc=(sid<<32)|(did<<16)|0x31 if sid else 0x11
            tlb=(did<<16)|0xe2 if sid else 0xd2
            if desc!=[f"context-cache high 0x0 low {cc:#x}", f"iotlb high 0x0 low {tlb:#x}",f"wait high {addr:#x} low {cookie<<32|0x25:#x}"]:
                raise ValueError("Acquittement sans contexte et drainage individualisés")
            desc.clear(); waits.append((n,addr,cookie))
        if line.startswith("vtd_reg_write_gcmd "):
            commands.append(int(line.split(" value ")[1],16))
        if line.startswith(("vtd_reg_ir_root","vtd_dmar_fault","vtd_ir_enable enable 1")):
            raise ValueError(f"État IR ou faute VT-d ligne {n}")
        if line.startswith("pci_cfg_write "):
            m=re.fullmatch(r"pci_cfg_write \S+ 00:([0-9a-f]+)\.(\d) @(0x[0-9a-f]+) <- (0x[0-9a-f]+)",line)
            if not m: raise ValueError("Écriture PCI non décodée")
            slot,fn,off,v=[int(x,16) for x in m.groups()]
            sid=8*slot+fn
            if sid not in (0,48,56,248,251) or off<4: raise ValueError("Topologie PCI")
            if off==4:
                on=bool(v&4)
                if on and (sid not in enabled or len(waits)<(2 if sid==48 else 3) or
                           v not in ((0x406,) if sid==48 else (6,0x406)) or commands[-1:]!=[0x84000000]):
                    raise ValueError("Armement avant confinement ou mode d'interruption inconnu")
                if sid in enabled and on!=enabled[sid]:
                    enabled[sid]=on; writes.append((n,sid,on))
        if line.startswith("virtqueue_fill "): fills.append(n)
        if line.startswith("pci_nvme_io_cmd "): submits.append(n)
    if len(waits)!=5 or desc or len({w[1] for w in waits})!=1 or commands!=[0x40000000,0x04000000,0x84000000]:
        raise ValueError("Racine ou file commune divergente")
    for sid in (48,56):
        change=[(n,on) for n,s,on in writes if s==sid]
        if len(change)!=2 or not change[0][1] or change[1][1] or not waits[order.index(sid)][0]<change[0][0]<change[1][0]<waits[len(order)-1-order[::-1].index(sid)][0]:
            raise ValueError("Armement et retrait du périphérique")
    overlap=sum(submits[0]<n<submits[-1] for n in fills) if submits else 0
    if overlap==0 or not any(fills[0]<n<fills[-1] for n in submits):
        raise ValueError("Aucune progression matérielle entrecroisée des deux périphériques")
    expected=[(str(sid),str(1 if sid==48 else 3),str(driver if sid==48 else netpid),"1","true" if i<3 else "false",str(i+1),str(48*(i+1))) for i,sid in enumerate(order) if sid]
    receipts=re.findall(r"DUO WAIT sid=(\d+) did=(\d+) instance=(\d+) generation=(\d+) publication=(\w+) cookie=(\d+) iqt=(\d+)",serial)
    if receipts!=expected: raise ValueError("Ordre ou identité des reçus")
    if mode.startswith("nvme-contention-"):
        disk_first=mode.endswith("-disque")
        collision=re.search(r"DUO contention demandes=2 premier_disque=(true|false) dma=40 octets_slots=(\d+) octets_mutex=(\d+)",serial)
        if not collision or (collision[1]=="true")!=disk_first: raise ValueError("Deux demandes en attente")
        for who,pid in (("NIC",netpid),("NVMe",driver)):
            msg=f"DUO demande {who} pid={pid} generation=1 doublon=refuse avant_WAIT=refuse"
            if serial.count(msg)!=1 or serial.index(msg)>collision.start(): raise ValueError("Dépôt et saturation par titulaire")
        withdrawals=list(re.finditer(r"DUO WAIT sid=(\d+) did=\d+ instance=\d+ generation=1 publication=false",serial))
        if len(withdrawals)!=2 or withdrawals[0].start()<collision.end(): raise ValueError("WAIT antérieur à la concurrence")
        released=re.findall(r"DUO restitution (NIC|NVMe) dma=(\d+)",serial)
        if released!=([("NVMe","34"),("NIC","8")] if disk_first else [("NIC","14"),("NVMe","8")]):
            raise ValueError("Restitution sans le bon acquittement")
        for wait,(device,pages) in zip(withdrawals,released):
            if serial.index(f"DUO restitution {device} dma={pages}")<wait.start(): raise ValueError("Restitution prématurée")
        if serial.count(f"DUO réponse NIC collectée pid={netpid} generation=1")!=1: raise ValueError("Réponse au titulaire")
    return dict(recus=receipts,waits=waits,bus_master=writes,commandes_vtd=commands,
                completions_nic_pendant_nvme=overlap,io_nvme_pendant_nic=sum(fills[0]<n<fills[-1] for n in submits))


def verifier(out, mode, sector, reader, initial, old):
    traffic = mode.startswith("nvme-contention-") or mode in ("nvme-charge", "nvme-charge-disque", "nvme-mesure-charge")
    measured = mode.startswith("nvme-mesure")
    label = "update" if traffic or measured else mode.removeprefix("nvme-")
    image = out / "namespace.raw"
    trace = (out / "qemu.trace").read_text()
    serial = (out / "serie.log").read_text()
    if mode == "nvme-cycles":
        from virtio_net_duo_cycles import verifier_cycles
        return verifier_cycles(out, sector, reader, initial, old)
    operations = io_trace(out / "qemu.trace", sector)
    result = {"secteur": sector, "mode": label, "sha256_avant": initial,
              "sha256_apres": digest(image), "sha256_lecteur": digest(reader), "operations": operations}
    writes_ir=re.findall(r"^vtd_reg_write addr (0x[0-9a-f]+) size (0x[0-9a-f]+) value (0x[0-9a-f]+)$",trace,re.M)
    if writes_ir and any(int(addr,16)<0xc0 and int(addr,16)+int(size,16)>0xb8 for addr,size,value in writes_ir):
        raise ValueError("Écriture IRTA durant le profil de stockage")
    if traffic and (not writes_ir or "vtd_reg_ir_root" in trace or "vtd_ir_enable enable 1" in trace):
        raise ValueError("Enregistrement des états effectifs IR incomplet ou incompatible")
    processes(result,serial,label,1,sector)
    expected = "ancien" if label == "init" else "nouveau"
    result["p9"] = inspect(reader,image,sector)
    if result["p9"]["etat"] != expected:
        raise ValueError("Contenu publié inattendu")
    if measured:
        if (serial.count("NVME P9 128 transactions WRITE SYNC READ conformes")!=1 or
            result["p9"]["generation"]!=old["generation"]+128):
            raise ValueError("128 publications distinctes et leur contenu final non confirmés")
        metrics=re.findall(r"DUO mesures hz=(\d+) duree=(\d+) octets=(\[.*?\]) service=(\[.*?\]) flush=(\d+)",serial)
        if len(metrics)!=1: raise ValueError("Mesures de temps absentes")
        hz,elapsed,raw_bytes,raw_service,number=metrics[0]
        samples=[c for group in re.findall(r"DUO FLUSH cycles=(\[.*?\])",serial) for c in ast.literal_eval(group)]
        hz=int(hz); elapsed=int(elapsed)
        sizes=ast.literal_eval(raw_bytes); service=ast.literal_eval(raw_service)
        if (len(samples)!=int(number) or len(samples)!=sum(op["op"]=="FLUSH" for op in operations) or
            len(samples)<256 or hz<=0 or min(samples)<=0 or sizes!=[sum(op.get("blocs",0)*sector for op in operations if op["op"]==kind) for kind in ("READ","WRITE")] or
            sum(samples)!=service[2]):
            raise ValueError("Mesures invité divergentes des commandes matérielles")
        result["mesures"]=dict(hz=hz,cycles=elapsed,cycles_par_flush=samples,octets=sizes,
            debit_Mio_s_sur_parcours=[size*hz/elapsed/(1<<20) for size in sizes],
            debit_Mio_s_sur_service=[size*hz/t/(1<<20) for size,t in zip(sizes,service)],
            flush_us={p:sorted(samples)[math.ceil(len(samples)*p/100)-1]*1e6/hz for p in (50,95,99,100)})
    elif label == "update":
        verify_transition(old,result["p9"],{"nouveau"})
        result["ordre_applicatif"] = application_order(chronology(operations,old,sector),serial)
    if label == "recover" and (result["sha256_apres"] != initial or
                               "NVME P9 oracle nouveau" not in serial or any(o["op"]!="READ" for o in operations)):
        raise ValueError("Remontage à froid sans écriture non confirmé")
    if label != "recover" and not {"READ", "WRITE", "FLUSH"}.issubset({o["op"] for o in operations}):
        raise ValueError("Le jeu de commandes NVMe est incomplet")
    if any(op.get("cqe") != 0 for op in operations):
        raise ValueError("Commande sans CQE de succès")
    if traffic:
        result.update(verifier_trafic(trace,serial,mode,result["processus"]["pilote"]))
        return result
    writes, waits, commands = [], [], []
    armed = False
    for n,line in enumerate(trace.splitlines(),1):
        if line.startswith("vtd_reg_write_gcmd"):
            m=re.fullmatch(r"vtd_reg_write_gcmd status (0x[0-9a-f]+) value (0x[0-9a-f]+)",line)
            if not m or int(m[2],16) & ~0xc4000000: raise ValueError(f"Commande VT-d imprévue ligne {n}")
            commands.append((n,int(m[2],16)))
        if line.startswith("vtd_inv_desc_wait_sw"):
            m=re.fullmatch(r"vtd_inv_desc_wait_sw wait invalidate status write addr (0x[0-9a-f]+) data (0x[0-9a-f]+)",line)
            if not m: raise ValueError("WAIT matériel non décodé")
            waits.append((n,int(m[1],16),int(m[2],16)))
        if line.startswith("pci_cfg_write "):
            m = re.fullmatch(r"pci_cfg_write \S+ 00:([0-9a-f]+)\.(\d) @(0x[0-9a-f]+) <- (0x[0-9a-f]+)",line)
            if not m: raise ValueError(f"Écriture PCI ambiguë ligne {n}")
            slot,fn,off,value=map(lambda x:int(x,16),m.groups())
            sid=slot*8+fn
            if sid not in (0,48,56,248,251) or off<4: raise ValueError("Topologie variable")
            if off==4 and value & 4:
                if sid != 48 or value!=0x406 or armed or len(waits)!=2 or commands[-1][1]!=0x84000000:
                    raise ValueError(f"Ouverture imprévue ligne {n}")
                armed=True
                writes.append((n,"armement",sid))
            elif sid==48 and off==4 and armed:
                if value!=0x402: raise ValueError("Fermeture PCI non confirmée")
                armed=False
                writes.append((n,"fermeture",sid))
    if (armed or len(writes)!=2 or [w[2] for w in waits]!=[1,2,3] or len({w[1] for w in waits})!=1 or
        not waits[1][0]<writes[0][0]<writes[1][0]<waits[2][0] or
        [c[1] for c in commands]!=[0x40000000,0x04000000,0x84000000]):
        raise ValueError("Ordre de la traduction, du bail et du retrait incohérent")
    driver=result["processus"]["pilote"]
    receipts=re.findall(r"DUO WAIT sid=(\d+) did=(\d+) instance=(\d+) generation=(\d+) publication=(\w+) cookie=(\d+) iqt=(\d+)",serial)
    if receipts!=[("48","1",str(driver),"1","true","2","96"),("48","1",str(driver),"1","false","3","144")]:
        raise ValueError("Reçus non liés à l'instance effective du pilote")
    result.update(bus_master=writes,waits=waits,commandes_vtd=commands,recus=receipts)
    return result
