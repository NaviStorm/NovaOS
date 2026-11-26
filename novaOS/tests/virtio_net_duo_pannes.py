"""Preuves Q35 de progression du domaine indépendant après une mort Ring 3."""
import ast
import re
import struct

from nvme_phase2 import application_order, processes
from nvme_phase2_oracle import chronology, inspect, io_trace, unconfirmed_write, verify_transition
from virtio_net_confinement import digest, reg
from virtio_net_duo_cycles import require, resources


def tables_finales(vm, qt, serial, mode):
    rows = re.findall(rb"DUO inventaire controleur=(\[.*?\]) disque=(\[.*?\]) reseau=(\[.*?\])", serial)
    require(len(rows)==1,"Inventaire avant armement absent")
    ctl, disk, nic = [ast.literal_eval(v.decode()) for v in rows[0]]
    addresses=ctl+disk+nic
    require([len(ctl),len(disk),len(nic)]==[8,6,26] and len(set(addresses))==40 and
            all(0<p<=256*1024**2-4096 and p%4096==0 for p in addresses),"Identité ou disjonction des cadres")
    def table(pa):
        qt.write(f"read {pa:#x} 4096\n".encode())
        row=qt.readline().strip()
        require(re.fullmatch(rb"OK 0x[0-9a-fA-F]{8192}",row),"Table VT-d tronquée")
        return {i:w for i,w in enumerate(struct.unpack("<512Q",bytes.fromhex(row[5:].decode()))) if w}
    def register(offset):
        addr=0xfed90000+offset
        return reg(vm,addr) | (reg(vm,addr+4)<<32)
    require(register(0x20)==ctl[0] and register(0x90)==ctl[6] and
            register(0x80)==register(0x88)==192 and reg(vm,ctl[7])==4,"File QI ou reçu final")
    require(table(ctl[0])=={0:ctl[1]|1},"Racine VT-d")
    network=mode=="nvme-panne-nic"
    sid,did,addr=(56,3,0x10000000) if network else (48,1,0x02000000)
    pages=nic[:4] if network else ctl[2:6]
    leaves=nic[4:] if network else disk
    perms=([1,1,2,1,1,2]+[2]*8+[1]*8) if network else [1,2,1,2,2,1]
    require(table(ctl[1])=={sid*2:pages[0]|1,sid*2+1:(did<<8)|2},"Contexte du bail retenu")
    for i,shift in enumerate((39,30,21)):
        require(table(pages[i])=={(addr>>shift)&511:pages[i+1]|3},"Table intermédiaire du bail retenu")
    require(table(pages[3])=={i:pa|perm for i,(pa,perm) in enumerate(zip(leaves,perms))},"Traductions ou permissions en quarantaine")
    return dict(controleur=ctl,disque_initial=disk,nic_initial=nic,retenus=nic if network else disk,
                sid=sid,did=did,permissions=perms,cookie=4,iqh=192,iqt=192)


def verifier_panne(out, mode, sector, reader, seed, old):
    network=mode=="nvme-panne-nic"
    failed,survives=(56,48) if network else (48,56)
    log=(out/"serie.log").read_text()
    trace=(out/"qemu.trace").read_text().splitlines()
    result=dict(panne="NIC" if network else "NVMe",secteur=sector,volume_initial=digest(seed))
    processes(result,log,"update" if network else "driver-death",1,sector)
    disk=result["processus"]["pilote"]
    match=re.findall(r"NET Q35 pilote pid=(\d+) mode=3 racines=1 generation=1",log)
    require(len(match)==1,"Nombre de pilotes réseau")
    nic=int(match[0])
    roots=[(int(p),int(n)) for p,n in re.findall(r"DUO IPC pid=(\d+) racines=(\d+)",log)]
    require(len(roots)==len(set(p for p,n in roots))==5 and max(n for p,n in roots)<=4 and nic!=disk,"Racines et titulaires")
    order=[0,48,56,survives]
    desc=[]; waits=[]; cmds=[]; bme={48:False,56:False}; edges={48:[],56:[]}
    for n,row in enumerate(trace):
        if row.startswith(("vtd_ir_enable enable 1","vtd_reg_ir_root","vtd_dmar_fault")):
            raise ValueError("Faute de confinement ou de livraison des interruptions")
        if row.startswith("vtd_reg_write addr "):
            m=re.match(r"vtd_reg_write addr (\S+) size (\S+)",row)
            off,size=int(m[1],16),int(m[2],16)
            require(not (off<0xc0 and off+size>0xb8),"Écriture IRTA")
        if row.startswith("vtd_reg_write_gcmd "): cmds.append(int(row.split(" value ")[1],16))
        if row.startswith("vtd_inv_desc invalidate desc type "): desc.append(row.split(" type ",1)[1])
        if row.startswith("vtd_inv_desc_wait_sw "):
            pa,c=int(row.split()[-3],16),int(row.split()[-1],16)
            require(c==len(waits)+1 and c<=4 and (not waits or pa==waits[0][1]),"Identité du WAIT")
            sid=order[c-1]; did={0:0,48:1,56:3}[sid]
            cc=(sid<<32)|(did<<16)|0x31 if sid else 0x11
            tlb=(did<<16)|0xe2 if sid else 0xd2
            require(desc==[f"context-cache high 0x0 low {cc:#x}",f"iotlb high 0x0 low {tlb:#x}",
                           f"wait high {pa:#x} low {(c<<32)|0x25:#x}"],"Portée du WAIT")
            waits.append((n,pa)); desc.clear()
        if row.startswith("pci_cfg_write "):
            m=re.fullmatch(r"pci_cfg_write \S+ 00:([0-9a-f]+)\.(\d) @(0x[0-9a-f]+) <- (0x[0-9a-f]+)",row)
            require(m is not None,"Écriture PCI inconnue")
            slot,fn,off,v=(int(x,16) for x in m.groups())
            sid=8*slot+fn
            require(sid in (0,48,56,248,251) and off>=4,"Périphérique Q35 inconnu")
            if off==4:
                on=bool(v&4)
                require(not on or (sid in bme and len(waits)>=order.index(sid)+1 and cmds[-1:]==[0x84000000]
                                      and v in ((0x406,) if sid==48 else (6,0x406))),"BME hors confinement")
                if sid in bme and bme[sid]!=on: edges[sid].append(n); bme[sid]=on
    require(len(waits)==4 and not desc and cmds==[0x40000000,0x04000000,0x84000000] and
            all(len(x)==2 for x in edges.values()) and not any(bme.values()) and
            max(edges[48][0],edges[56][0])<edges[failed][1]<edges[survives][1]<waits[-1][0],"Ordre matériel des fermetures")
    until=trace[edges[failed][1]+1:edges[survives][1]]
    receipts=re.findall(r"DUO WAIT sid=(\d+) did=(\d+) instance=(\d+) generation=1 publication=(true|false) cookie=(\d+) iqt=(\d+)",log)
    expected=[(str(s),str(1 if s==48 else 3),str(disk if s==48 else nic),"true" if i<4 else "false",str(i),str(i*48)) for i,s in enumerate(order[1:],2)]
    require(receipts==expected,"Reçus des instances et absence de reçu pour le bail en quarantaine")
    ops=io_trace(out/"qemu.trace",sector)
    image=out/"namespace.raw"
    final=inspect(reader,image,sector)
    if network:
        require(f"DUO panne NIC pid={nic} generation=1 BME=0 DMA_retenu=26 NVMe_verifie=true" in log and
                "NET L2 pilote fin=Exit(9) racines=0" in log and
                f"DUO réseau instance={nic} génération=1 drainage=Err(Quarantine)" in log,"Mort réelle et quarantaine NIC")
        cids=[int(row.split()[2]) for row in until if row.startswith("pci_nvme_io_cmd ")]
        after=[op for op in ops if op["cid"] in cids]
        require({op["op"] for op in after}=={"READ","WRITE","FLUSH"} and all(op.get("cqe")==0 for op in after),"E/S P9 après panne")
        verify_transition(old,final,{"nouveau"})
        result.update(publication=application_order(chronology(ops,old,sector),log),apres_panne=after)
    else:
        require("DUO panne NVMe BME=0 DMA_retenu=6 NIC_verifie=true" in log and
                result["processus"]["retours"][str(disk)]==0 and "NVME RW pilote mort apres WRITE" in log and
                "NET L2 pilote fin=Exit(0) racines=0" in log and
                f"DUO réseau instance={nic} génération=1 drainage=Ok(())" in log,
                "Mort sur WRITE et drainage du pilote NIC")
        lens=[int(re.search(r" len (\d+) ",row)[1]) for row in until if row.startswith("virtqueue_fill ")]
        require(lens.count(0)==32 and sum(n>=72 for n in lens)==32,"Deux lots matériels TX/RX après fermeture NVMe")
        verify_transition(old,final,{"ancien"})
        flushes=[op for op in ops[:-1] if op["op"]=="FLUSH"]
        require(len(flushes)==1 and flushes[0].get("cqe")==0 and
                log.index(f"NVME RW FLUSH CQE cid={flushes[0]['cid']} ecritures=0 numero=1")<
                log.index("NVME P9 debut transaction"),"FLUSH initial du montage")
        require(final["ancres"]==old["ancres"] and all(op["op"] in ("READ","FLUSH") and op.get("cqe")==0 for op in ops[:-1]) and ops[-1]["op"]=="WRITE",
                "Ancres ou admissions NVMe après perte du pilote")
        result.update(ecriture_non_confirmee=unconfirmed_write(seed,image,ops,sector),tx_apres=32,rx_apres=32)
    snapshots={p:resources(log,p) for p in ("duo_avant_panne","net_files_pleines","duo_panne_nic" if network else "duo_panne_disque","nvme_apres_retrait")}
    after=snapshots["nvme_apres_retrait"]
    require(after["dma"]==(34 if network else 14) and snapshots["net_files_pleines"]["dma"]==40 and
            all(after[k]==0 for k in ("ipc","processus","utilisateur","upile","meta_actifs")),"Ressources finales ou quarantaine")
    result.update(racines=roots,mesures=snapshots,recus=receipts,p9=final,bme_lignes=edges,wait_lignes=waits,volume_final=digest(image))
    return result
