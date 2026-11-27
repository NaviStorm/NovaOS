"""Oracles de retrait Ethernet du laboratoire 3C. Relecture des deux frontières DMA."""
from collections import Counter
import json
import re
import struct

NOUVEAUX = ("cycles-256", "cycles", "cycles-court", "rattrapage", "preuve-absente", "recu-vtd-absent",
            "recu-vtd-differe", "recu-arm-differe", "recu-arm-absent", "deces-recu-tx-absent")
CAS = {"cycles-256": 0, "port": 0, "cycles": 0, "court": 8, "cycles-court": 8, "preuve-absente": 8,
       "recu-vtd-absent": 8, "recu-vtd-differe": 8, "recu-arm-differe": 0, "recu-arm-absent": 0, "deces": 3, "deces-recu-tx-absent": 3, "incertain": 4, "revocation": 6,
       "segment-interrompu": 23, "rattrapage": 23, "recu-collecte": 24, "echo1514": 13}


RESET = {"mmio-absent":8, "mmio-differe":8, "mmio-invalide":8, "echo1514-reset":13, "reset-court":8, "reset-differe":8, "reset-absent":8, "reset-invalide":8,
         "reset-sans-option":8, "reset-iommu-absent":8, "reset-iommu-differe":8,
         "reset-deces":3, "reset-revocation":6, "reset-incertain":4}
NOUVEAUX += tuple(RESET)
CAS.update(RESET)


def exiger(ok, objet):
    if not ok:
        raise ValueError(f"Cycle de vie DMA : {objet}")


def nombre_cycles(cas, arch):
    if cas == "cycles-256" and arch == "arm":
        return 256
    if cas == "cycles" or (cas == "cycles-court" and arch == "q35"):
        return 4
    return 2 if cas in ("rattrapage", "recu-vtd-differe", "recu-arm-differe", "reset-court", "reset-iommu-differe", "mmio-differe") else 1


def pages_retenues(rx, tx):
    retenues = set(range(6)) if rx != 65535 or tx else set()
    for i in range(8):
        if (rx >> (2*i)) & 3 != 3:
            retenues.add(6+i)
        if (tx >> (2*i)) & 3:
            retenues.add(14+i)
    return retenues


def domaines_arm(out, retenues):
    """Contrôle les permissions dans la trace réelle et identifie chaque IOVA finale."""
    for phase in ("reset", "fin"):
        capture = json.loads((out / (phase + ".json")).read_text())
        exiger(not capture["vm"]["running"] and capture["boot_bypass"] is False
               and capture["iommu_platform"] and capture["pci_id_ecam"] == 0x10411af4
               and not capture["pci_command"] & 4, phase)
    domaines = {}
    cartes = {d: {} for d in (1,2,3)}
    compte = Counter()
    firmware = True
    master = False
    with (out / "qemu.trace").open() as log:
        for numero, line in enumerate(log, 1):
            line = line.strip()
            if line.startswith("pci_cfg_write virtio-net-pci 00:07.0 @0x4 "):
                master = bool(int(line.split()[-1], 16) & 4)
                if master and not firmware:
                    exiger(domaines.get(56) == 3 and len(cartes[3]) == 22, numero)
            elif line.startswith(("virtio_iommu_attach ", "virtio_iommu_detach ")):
                m = re.fullmatch(r"virtio_iommu_(attach|detach) domain=(\d+) endpoint=(\d+)", line)
                exiger(m, numero)
                op, d, sid = m[1], int(m[2]), int(m[3])
                exiger(d in cartes and {1:48,2:48,3:56}[d] == sid and not cartes[d], numero)
                if op == "attach":
                    exiger(sid not in domaines and (d == 2) == firmware, numero)
                    domaines[sid] = d
                else:
                    exiger(domaines.pop(sid, None) == d, numero)
                    if d == 2:
                        firmware = False
                compte[f"{op}_{d}"] += 1
            elif line.startswith(("virtio_iommu_map ", "virtio_iommu_unmap_done ")):
                m = re.fullmatch(r"virtio_iommu_(map|unmap_done) domain=(\d+) virt_start=(0x\w+) virt_end=(0x\w+)(?: phys_start=(0x\w+) flags=([123]))?", line)
                exiger(m, numero)
                d, lo, hi = int(m[2]), int(m[3],16), int(m[4],16)
                exiger(d in domaines.values() and lo % 4096 == 0 and (hi+1) % 4096 == 0 and hi >= lo, numero)
                if m[1] == "map":
                    pa, droits = int(m[5],16), int(m[6])
                    exiger(pa % 4096 == 0 and all(hi < a or lo > b for a,b in cartes[d]), numero)
                    if d != 2:
                        base = 0x8000000 if d == 1 else 0x10000000
                        slot = (lo-base)//4096
                        permis = [1,1,3,1,2,2,1] if d == 1 else [1,1,2,1,1,2]+[2]*8+[1]*8
                        exiger(hi == lo+4095 and 0 <= slot < len(permis) and permis[slot] == droits, numero)
                        exiger(all(val[0] != pa for c in cartes.values() for val in c.values()), numero)
                    cartes[d][lo,hi] = pa,droits
                else:
                    exiger(not (d == 3 and master) and cartes[d].pop((lo,hi),None) is not None, numero)
                compte[f"{m[1]}_{d}"] += 1
            elif line.startswith("virtio_iommu_translate "):
                m = re.fullmatch(r".*memory-region-\d+-(\d+) rid=(\d+) addr=(0x\w+) flag=([12])",line)
                exiger(m and m[1] == m[2], numero)
                d = domaines.get(int(m[2]))
                exiger(d in cartes and (d != 3 or master) and any(
                    lo <= int(m[3],16) <= hi and perm & int(m[4])
                    for (lo,hi),(_,perm) in cartes[d].items()), numero)
                compte[f"traductions_{d}"] += 1
            elif line.startswith(("virtio_iommu_report_fault", "virtio_iommu_set_config")):
                exiger(False, numero)
    finales = {(lo-0x10000000)//4096 for lo,_ in cartes[3]}
    exiger(not master and not firmware and not cartes[1] and not cartes[2] and finales == retenues,
           ("IOVA finales", finales, retenues))
    exiger(domaines == ({56:3} if retenues else {}) and all(compte[f"traductions_{d}"] for d in (1,2,3)), "attributions finales")
    return dict(compteurs=dict(compte), slots_retenus=sorted(finales), domaines_finaux=domaines)


def capacite_pci_arm(vm, serie):
    """Relit en ECAM la capacité PCI commune du seul NIC identifié, sans écrire le BAR."""
    from virtio_net_confinement import reg
    cfg = 0x4010000000 + (7 << 15)
    exiger(reg(vm, cfg) == 0x10411af4, "Identité PCI ARM")
    pos = reg(vm, cfg+0x34) & 255
    vus = set()
    commune = []
    while pos:
        exiger(0x40 <= pos <= 0xfc and pos % 4 == 0 and pos not in vus, "Chaîne des capacités PCI")
        vus.add(pos)
        debut = reg(vm,cfg+pos)
        if debut & 255 == 9 and debut >> 24 == 1:
            exiger(pos+16 <= 256 and (debut >> 16) & 255 >= 16, "Étendue de la capacité commune")
            commune.append((reg(vm,cfg+pos+4)&255, reg(vm,cfg+pos+8), reg(vm,cfg+pos+12)))
        pos = (debut >> 8) & 255
    exiger(len(commune)==1, "Capacité commune unique")
    bar,offset,taille = commune[0]
    features = re.findall(r"NET EVENT_IDX offre=(0x\w+) relu=(0x\w+)",serie)
    exiger(features and len(set(features))==1, "Négociation du NIC")
    offre,negocie = (int(v,16) for v in features[0])
    return dict(bar=bar,offset=offset,octets=taille,champ_queue_reset_dans_capacite=taille>=60,
                bit_40_offert=bool(offre & 1<<40),bit_40_negocie=bool(negocie & 1<<40))


def preuve_qi(vm, qt, serie, retire):
    """Relit les commandes QI en RAM et leur reçu pendant que le vCPU est arrêté."""
    from virtio_net_confinement import reg
    m = re.search(r"NET Q35 domaine=3 sid=56 .*?root=(0x\w+) qi=(0x\w+) recu=(0x\w+)", serie)
    exiger(m, "racine Q35 absente")
    root, queue, statut = [int(x,16) for x in m.groups()]
    def lire(adresse, taille):
        qt.write(f"read {adresse:#x} {taille}\n".encode())
        ligne = qt.readline().strip()
        exiger(re.fullmatch(rb"OK 0x[0-9a-fA-F]+",ligne), adresse)
        brut = bytes.fromhex(ligne[5:].decode())
        exiger(len(brut) == taille, adresse)
        return brut
    def registre64(offset):
        return reg(vm,0xfed90000+offset) | (reg(vm,0xfed90004+offset)<<32)
    head,tail = registre64(0x80),registre64(0x88)
    cookie = reg(vm,statut)
    exiger(head == tail == 48*cookie and 0 < cookie <= 9 and registre64(0x20) == root
           and registre64(0x90) == queue and reg(vm,0xfed90034) == 0, "registres QI")
    commandes=[]
    for i in range(cookie):
        mots = struct.unpack("<6Q",lire(queue+48*i,48))
        exiger(mots == (0x11,0,0xd2,0,((i+1)<<32)|0x25,statut), ("QI",i,mots))
        commandes.append(list(mots))
    vide = lire(root,4096) == bytes(4096)
    exiger(vide == retire, "racine VT-d finale")
    return dict(racine=root,queue=queue,statut=statut,iqh=head,iqt=tail,cookie=cookie,
                racine_vide=vide,descripteurs=commandes)

def verifier_reset(cas, c, serie, generations):
    actif = cas != "reset-sans-option"
    valide = actif and cas not in ("reset-invalide", "reset-absent", "mmio-absent", "mmio-invalide")
    pci = c["capacite_pci_arm"]
    exiger(pci["bar"] == 4 and pci["offset"] == 0 and pci["champ_queue_reset_dans_capacite"]
           and pci["bit_40_offert"] == actif and pci["bit_40_negocie"] == actif, pci)
    acquits = re.findall(r"E3E reset file=(\d+) pid=(\d+) generation=(\d+) queue_reset=0 queue_enable=0", serie)
    preuves = re.findall(r"E3E files pilote=(\d+) incarnation=(\d+) generation=(\d+) confirmees=(true|false)", serie)
    sessions = re.findall(r"ETH3 session .*?pilote=(\d+) .*?generation_materiel=(\d+)", serie)
    exiger(len(preuves) == (len(generations) if actif else 0), preuves)
    attendus = []
    for (pid, inc, gen, ok), session in zip(preuves, sessions):
        exiger((pid,gen) == session and int(inc) > 0 and (ok=="true") == valide, (session, preuves))
        attendus += [(f,pid,gen) for f in (["0"] if cas in ("reset-absent","mmio-absent","mmio-invalide") else ["0","1"])]
    exiger(acquits == attendus, acquits)
    if cas == "reset-absent":
        exiger("reset TX non transmis queue_enable=1" in serie, cas)
    if cas == "reset-differe":
        exiger("livraison reset TX differee" in serie, cas)
    if cas in ("mmio-absent", "mmio-invalide"):
        observation = ("incoherent=false attente=Err(Timeout)" if cas=="mmio-absent" else "incoherent=true attente=Ok(())")
        exiger("E3E reset non confirme file=1 " + observation in serie, (cas,observation))
    return valide


def verifier(out, c, serie):
    cas, arch = c["cas"], c["arch"]
    mode = CAS[cas]
    cycles = nombre_cycles(cas,arch)
    admissions = re.findall(r"E3C admission generation=(\d+) precedente=(\d+) ancienne_capacite_valide=false", serie)
    generations = [int(n) for n,_ in admissions]
    exiger(len(generations) == cycles and all(int(g)>int(pre) for g,pre in admissions)
           and [int(pre) for _,pre in admissions] == [0]+generations[:-1], admissions)
    sessions = re.findall(r"ETH3 session .*?incarnation=(\d+) .*?epoque=(\d+) generation_materiel=(\d+) mode=(\d+) racines=\[0, 4, 1\] boites_globales=2 endpoints_globaux=5 ipc_pages=9", serie)
    exiger(len(sessions)==cycles and [int(s[2]) for s in sessions]==generations
           and all(int(s[3])==mode for s in sessions)
           and all(len({s[i] for s in sessions})==cycles for i in (0,1)), sessions)
    arrets = re.findall(r"E3C arret generation=(\d+) reset=0 queues=0,0 bme=0 ouverture=true rx_lus=(\d+) tx_lus=(\d+) tx_publies=(\d+) rx_rendus=(0x\w+) tx_en_vol=(0x\w+)",serie)
    absent = cas == "preuve-absente"
    exiger(len(arrets)==(0 if absent else cycles),arrets)
    if absent:
        exiger(f"E3C preuve de reset retenue generation={generations[-1]}" in serie,cas)
    else:
        exiger([int(a[0]) for a in arrets]==generations,arrets)
    bilans = re.findall(r"ETH3 bilan mode=(\d+) materiel=(\S+) retrait=(\S+) acteurs=\[Exit\((\d+)\), Exit\((\d+)\), Exit\((\d+)\)\] appels_pilote=(\d+)",serie)
    exiger(len(bilans)==cycles and all(int(b[0])==mode for b in bilans),bilans)
    attentes={0:144,8:1,3:4,4:4,6:4,23:0,24:8,13:13312}
    exiger(c["trames_par_sens"] == cycles*attentes[mode] and c["erreur_pair"] is None, c["trames_par_sens"])
    if cas in ("recu-arm-differe", "recu-vtd-differe", "recu-arm-absent", "reset-iommu-absent", "reset-iommu-differe"):
        differes = re.findall(r"E3C retrait differe (VirtIO-IOMMU|VT-d) generation=(\d+) (?:numero|cookie)=(\d+) anciennes_references_refusees=true epingles(?:_nic)?=(\d+)", serie)
        exiger(len(differes)==cycles and [int(d[1]) for d in differes]==generations
               and len({d[2] for d in differes})==cycles and
               all((d[0], int(d[3]))==(("VirtIO-IOMMU",22) if arch=="arm" else ("VT-d",30)) for d in differes), differes)
    if cas in ("recu-arm-differe", "recu-arm-absent", "reset-iommu-absent", "reset-iommu-differe"):
        exiger(serie.count("epingles_nic=22 lecture_flush=true") == cycles, "E/S pendant la quarantaine")
    if cas=="deces-recu-tx-absent":
        exiger("E3C recu TX retenu" in serie and arrets[0][2:4]==("0","4") and int(arrets[0][5],16)==15, arrets)
    if cas=="rattrapage":
        exiger(c["rx_injectees_perf"]==16*cycles and all(int(a[4],16)==65535 and int(a[1])==0 for a in arrets), arrets)
    for b in bilans:
        sorties = tuple(map(int,b[3:6]))
        if mode in (0,8,13):
            exiger(sorties==(0,0,0) and b[1]=="Ok(())",b)
        elif mode==23:
            exiger(sorties[2]==9 and 125 not in sorties and int(b[6])==1,b)
        else:
            exiger(sorties==({3:9,4:11,6:10,24:9}[mode],0,0),b)
        exiger((b[2]=="Err(12)" if absent else b[2]=="Ok(0)"),b)
    ram={}
    for phase,items in re.findall(r"RAM phase=(\w+) ([^\n]+)",serie):
        ram.setdefault(phase,[]).append({k:int(v) for k,v in re.findall(r"(\w+)=(\d+)",items)})
    cpu = [list(map(int,x)) for x in re.findall(r"ETH3 cadres avant=(\d+) apres=(\d+)",serie)]
    exiger(len(cpu)==cycles and all(a==b for a,b in cpu),cpu)
    exiger(all(v["ipc"]==0 and v["processus"]==0 and v["upile"]==0 for v in ram["eth3_retrait"]), "ressources acteurs")
    evidence = dict(generations=generations,incarnations=sessions,arrets=arrets,bilans=bilans,
                    cadres_cpu=cpu,ram=ram)
    if arch=="arm":
        retenues = set(range(22)) if absent else pages_retenues(int(arrets[-1][4],16),int(arrets[-1][5],16))
        confirme = False
        if cas in RESET:
            confirme = verifier_reset(cas, c, serie, generations)
            if confirme:
                retenues = set()
        sans_livraison = cas in ("recu-arm-absent", "reset-iommu-absent")
        if sans_livraison:
            retenues = set(range(22))
        if not absent and not sans_livraison:
            retrait=re.findall(r"E3C VirtIO-IOMMU generation=(\d+) rendus=(\d+) quarantaine=(\d+) detache=(true|false)",serie)
            exiger(len(retrait)==cycles,retrait)
            for a,s in zip(arrets,retrait):
                n=0 if confirme else len(pages_retenues(int(a[4],16),int(a[5],16)))
                exiger(a[0]==s[0] and int(s[1])==22-n and int(s[2])==n and (s[3]=="true")==(n==0),s)
        if retenues:
            exiger(f"rattachement refuse generation={generations[-1]}" in serie and
                   f"ETH3 quarantaine disque_lisible=true cadres_retenus={len(retenues)+9}" in serie, "quarantaine ARM")
        else:
            exiger("NET ARM bilan Ok(())" in serie and "NET ARM L2 retrait disque_lisible=true cadres_fin=0" in serie,"restitution ARM")
        exiger(c["volume_relu_identique"], "P9 modifié")
        evidence["iommu"]=domaines_arm(out, retenues - {0,21} if sans_livraison else retenues)
        evidence["memoire_finale"]=dict(nic=len(retenues),controleur=9 if retenues else 0,permanent=0)
    else:
        qi=c["preuve_qi"]
        manque = cas=="recu-vtd-absent"
        if absent or manque:
            exiger("NET Q35 etat=Err(" in serie and qi["racine_vide"]==manque,cas)
            exiger(qi["cookie"]==(3 if manque else 2),qi)
            exiger("rattachement_refuse=true" in serie, "propriétaire de quarantaine")
            if manque:
                exiger("cookie=3 epingles=30" in serie, "reçu retenu")
        else:
            exiger("NET Q35 etat=Ok(())" in serie and qi["racine_vide"] and qi["cookie"]==1+2*cycles,qi)
            rec = re.findall(r"E3C VT-d generation=(\d+) bme=0 reset=0 cc=global iotlb=global dr=1 dw=1 iqh=(\d+) iqt=(\d+) cookie=(\d+) fsts=0",serie)
            exiger(len(rec)==cycles and all(int(g)==i+1 and int(cookie)==3+2*i and head==tail
                   and int(head)==48*int(cookie) for i,(g,head,tail,cookie) in enumerate(rec)),rec)
        evidence["memoire_finale"]=dict(nic=22 if absent or manque else 0,tables_privees=5 if absent or manque else 0,permanent=3)
    return evidence
