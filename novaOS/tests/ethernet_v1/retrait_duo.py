"""Oracles indépendants de la phase 3D pour la QI commune et les deux baux."""
from ast import literal_eval
import re
import struct

CAS = {"duo-retrait": "nvme-panne-nic", "duo-differe": "nvme-panne-nic",
       "duo-absent": "nvme-panne-nic", "duo-sans-reset": "nvme-panne-nic",
       "duo-reouverture": "nvme-panne-nic", "duo-cycles": "nvme-cycles", "duo-concurrence": "nvme-contention-nic",
       "duo-disque": "nvme-contention-disque"}


def exiger(valeur, raison):
    if not valeur:
        raise ValueError(raison)


def nombre_cycles(cas):
    return {"duo-cycles": 256, "duo-reouverture": 8}.get(cas, 1)


def sequence(cas):
    if cas in ("duo-sans-reset", "deces"):
        return [48, 56, 48]
    if cas == "duo-disque":
        return [48, 56, 48, 56]
    return [48, 56, 56, 48] * nombre_cycles(cas)


def commandes(sid, cookie, adresse):
    did = {0: 0, 48: 1, 56: 3}[sid]
    cc = (sid << 32 | did << 16 | 0x31) if sid else 0x11
    tlb = (did << 16 | 0xe2) if sid else 0xd2
    return [(cc, 0), (tlb, 0), (cookie << 32 | 0x25, adresse)]


def lire_page(qt, pa):
    qt.write(f"read {pa:#x} 4096\n".encode())
    ligne = qt.readline().strip()
    exiger(re.fullmatch(rb"OK 0x[0-9a-fA-F]{8192}", ligne), "Lecture QTest de page")
    return struct.unpack("<512Q", bytes.fromhex(ligne[5:].decode()))


def temoin_actif(vm, qt, journal):
    from virtio_net_confinement import reg
    from virtio_net_pio_q35 import qt_command
    inventaire = re.findall(rb"DUO inventaire controleur=(\[.*?\]) disque=(\[.*?\]) reseau=(\[.*?\])", journal)
    exiger(len(inventaire) == 1, "Inventaire des baux avant armement")
    ctl, disque, nic = [literal_eval(v.decode()) for v in inventaire[0]]
    cadres = ctl + disque + nic
    exiger([len(ctl), len(disque), len(nic)] == [8, 6, 26] and len(set(cadres)) == 40
           and all(0 < pa <= 256*1024**2-4096 and pa % 4096 == 0 for pa in cadres), "Disjonction physique")

    def table(pa):
        return {i:v for i,v in enumerate(lire_page(qt, pa)) if v}

    def arbre(pages, iova):
        for i, decalage in enumerate((39, 30, 21)):
            exiger(table(pages[i]) == {(iova >> decalage) & 511:pages[i+1]|3}, "Table intermédiaire")
        return table(pages[3])

    cf8 = qt_command(qt, "inl 0xcf8")
    bme = {}
    try:
        for sid in (48, 56):
            qt_command(qt, f"outl 0xcf8 {0x80000004 | (sid << 8):#x}")
            bme[sid] = bool(qt_command(qt, "inw 0xcfc") & 4)
    finally:
        qt_command(qt, f"outl 0xcf8 {cf8:#x}")
    exiger(bme == {48:True,56:False} and reg(vm, 0xfed9001c) == 0xc4000000
           and reg(vm, 0xfed90034) == 0 and reg(vm, ctl[7]) == 4
           and reg(vm, 0xfed90080) == reg(vm, 0xfed90088) == 192, "Barrière sélective ou BME")
    exiger(table(ctl[0]) == {0:ctl[1]|1} and table(ctl[1]) == {96:ctl[2]|1,97:0x102},
           "Contexte NVMe seul dans la racine active")
    feuilles = arbre(ctl[2:6], 0x02000000)
    permis = [1,2,1,2,2,1]
    exiger(set(range(4)) <= set(feuilles) <= set(range(6))
           and all(v == disque[i] | permis[i] for i,v in feuilles.items()), "Feuilles NVMe actives")
    droits_nic = [1,1,2,1,1,2] + [2]*8 + [1]*8
    exiger(arbre(nic[:4], 0x10000000) == {i:p|d for i,(p,d) in enumerate(zip(nic[4:],droits_nic))},
           "Propriété des 26 cadres retenus avant livraison")
    return dict(bme=bme, controleur=ctl, disque=disque, nic=nic,
                feuilles_disque=feuilles, permissions_nic=droits_nic, cookie=4, iqh=192, iqt=192)


def materiel(vm, qt, journal, cas):
    from virtio_net_confinement import reg

    def registre(offset):
        return reg(vm, 0xfed90000 + offset) | (reg(vm, 0xfed90004 + offset) << 32)

    ancres = {tuple(int(v, 16) for v in valeurs) for valeurs in
              re.findall(rb"DUO tables root=(0x\w+) qi=(0x\w+) wait=(0x\w+)", journal)}
    exiger(len(ancres) == 1, "Ancrage commun instable")
    root, qi, statut = ancres.pop()
    serielles = [0] + sequence(cas)
    cookie = len(serielles)
    queue = (cookie * 48) % 4096
    racine = lire_page(qt, root)
    ctx = racine[0] & ~4095
    exiger(len({root, qi, statut, ctx}) == 4 and all(0 < a <= 256*1024**2-4096 and a % 4096 == 0
           for a in (root, qi, statut, ctx)), "Ancres distinctes en RAM")
    exiger(racine == (ctx | 1,) + (0,)*511 and reg(vm, 0xfed9001c) == 0xc4000000
           and reg(vm, 0xfed90034) == 0 and registre(0x20) == root and registre(0x90) == qi
           and registre(0x80) == registre(0x88) == queue and reg(vm, statut) == cookie,
           "Racine, état DRHD, indices ou cookie QTest")
    attendus = [0] * 512
    for i, sid in enumerate(serielles):
        for j, (bas, haut) in enumerate(commandes(sid, i+1, statut)):
            n = (i*3 + j) % 256
            attendus[2*n:2*n+2] = bas, haut
    exiger(tuple(attendus) == lire_page(qt, qi), "Descripteurs réellement présents dans la file QI")
    contexte = {i: v for i, v in enumerate(lire_page(qt, ctx)) if v}
    if cas == "duo-sans-reset":
        exiger(set(contexte) == {112, 113} and contexte[113] == 0x302 and contexte[112] & 4095 == 1,
               "Contexte NIC conservé sans preuve")
    else:
        exiger(not contexte, "Contexte présent après les deux retraits")
    return dict(racine=root, contexte=ctx, queue=qi, page_recu=statut, cookie=cookie, iqt=queue,
                bouclages=cookie*48//4096, contextes_finaux=contexte)


def verifier(chemin, cas, lecteur, avant, pair):
    from nvme_phase2 import application_order
    from nvme_phase2_oracle import inspect, io_trace, chronology, verify_transition
    from virtio_net_duo_cycles import resources
    serie = (chemin / "serie.log").read_text()
    trace = (chemin / "qemu.trace").read_text().splitlines()
    cycles = nombre_cycles(cas)
    mort = CAS[cas] == "nvme-panne-nic"
    retenus = 26 if cas in ("duo-sans-reset", "duo-absent") else 0
    exiger("NVME bilan ok=true" in serie and not pair.error
           and pair.frames == (4 if mort else 144)*cycles, "Fin du superviseur ou équivalence des trames")
    exiger(f"NVME ressources initial=0 final={8+retenus} vcpus=1" in serie, "Cadres DMA finaux")
    final = inspect(lecteur, chemin/"namespace.raw", 512)
    if cycles == 1:
        verify_transition(avant, final, {"nouveau"})
    exiger(final["etat"] == "nouveau" and final["generation"] == avant["generation"]+cycles,
           "Relecture indépendante des ancres et du fichier P9")
    ops = io_trace(chemin/"qemu.trace", 512, cycles)
    attendus = [0] + sequence(cas)
    journal_qi = re.findall(r"DUO WAIT sid=(\d+) did=(\d+) instance=(\d+) generation=(\d+) publication=(true|false) cookie=(\d+) iqt=(\d+)", serie)
    exiger(len(journal_qi) == len(attendus)-1, "Nombre de reçus du superviseur")
    for i, (sid, did, pid, g, pub, marque, fin) in enumerate(journal_qi, 2):
        exiger(int(sid) == attendus[i-1] and int(did) == {48:1, 56:3}[int(sid)]
               and int(marque) == i and int(fin) == (48*i)%4096 and int(pid)>0
               and int(g)==(i-2)//4+1 and (pub=="true")==((i-2)%4<2), "Titulaire ou indices du reçu")
    commandes_qi = []
    adresses = set()
    waits = []
    gcmd = []
    active = {48:False, 56:False}
    fenetres = []
    apres = []
    for index, ligne in enumerate(trace):
        if ligne.startswith(("vtd_dmar_fault", "vtd_reg_ir_root", "vtd_ir_enable enable 1")):
            raise ValueError("Faute VT-d ou modification IRTA")
        if ligne.startswith("vtd_reg_write_gcmd "):
            gcmd.append(int(ligne.split(" value ")[1],16))
        mm = re.fullmatch(r"pci_cfg_write \S+ 00:0([67])\.0 @0x4 <- (0x\w+)",ligne)
        if mm:
            active[int(mm[1])*8] = bool(int(mm[2],16)&4)
        if ligne.startswith("vtd_inv_desc invalidate desc type "):
            commandes_qi.append(ligne.split(" type ")[1])
        if ligne.startswith("vtd_inv_desc_wait_sw "):
            adresse, cookie = [int(ligne.split()[k],16) for k in (-3,-1)]
            exiger(cookie==len(waits)+1 and cookie<=len(attendus), "Séquence matérielle QI")
            batch = commandes(attendus[cookie-1],cookie,adresse)
            exiger(commandes_qi == [f"{nom} high {haut:#x} low {bas:#x}" for nom,(bas,haut) in
                   zip(("context-cache","iotlb","wait"),batch)], "Portée matérielle des invalidations")
            if mort and cookie%4==0 and cas!="duo-sans-reset":
                exiger(active=={48:True,56:False}, "BME NVMe pendant le retrait individuel")
            commandes_qi.clear()
            waits.append(index)
            adresses.add(adresse)
        if ligne.startswith("pci_nvme_io_cmd ") and active[48] and not active[56] and len(waits)>=3:
            fenetres.append(ligne.split(" opc ")[1].split()[0])
            if len(waits)%4==0:
                apres.append(fenetres[-1])
    exiger(len(waits)==len(attendus) and len(adresses)==1 and not commandes_qi
           and gcmd==[0x40000000,0x04000000,0x84000000], "Maintien de la racine et des traductions")
    if mort:
        exiger(set(fenetres)=={"0x0","0x1","0x2"}, "READ/WRITE/FLUSH après désarmement NIC")
        if cas!="duo-sans-reset":
            exiger(set(apres)==set(fenetres) and "retenus=26 anciennes=refusees admission=refusee" in serie,
                   "E/S après WAIT ciblé et refus d'admission")
            collecte=re.findall(r"E3D collecte NIC .*? terminees=(\d+) liberes=26 doublon=refuse NVMe_actif=true",serie)
            exiger(collecte==([] if retenus else ["30" if cas=="duo-differe" else "1"]*cycles), "Collecte unique")
        else:
            exiger("DMA_retenu=26 NVMe_verifie=true" in serie and "E3D WAIT NIC" not in serie, "Absence du jeton")
    if cas in ("duo-concurrence","duo-disque"):
        exiger("DUO contention demandes=2" in serie and "DUO réponse NIC collectée" in serie, "Retraits concurrents")
    return bilan_cycles(serie, ops, avant, final, cycles, mort, retenus, application_order, chronology, resources) | {
        "attentes": len(waits), "E_S_apres_BME_NIC": len(fenetres), "E_S_apres_WAIT_NIC": len(apres)}


def bilan_cycles(serie, ops, avant, final, cycles, mort, retenus, application_order, chronology, resources):
    lignes = []
    pids = set()
    slot = avant["slot"]
    for g in range(1,cycles+1):
        log = serie if cycles==1 else serie.split(f"DUO debut cycle={g}\n",1)[1].split(f"DUO fin cycle={g}\n",1)[0]
        io = [o for o in ops if o.get("epoque",1)==g]
        exiger(io and all(o.get("cqe")==0 for o in io), f"CQE génération {g}")
        ordre = application_order(chronology(io,{"slot":slot},512),log)
        slot = 1-slot
        racines = {int(p):int(n) for p,n in re.findall(r"DUO IPC pid=(\d+) racines=(\d+)",log)}
        exiger(len(racines)==7 and max(racines.values())<=4 and not pids.intersection(racines),
               f"Identité ou droits des sept acteurs génération {g}")
        pids.update(racines)
        session = re.search(r"ETH3 session .*?pilote=(\d+).*?generation_materiel=(\d+).*?racines=\[0, 4, 1\] boites_globales=(\d+)",log)
        exiger(session and int(session[2])==g and int(session[3])<=8, f"Session Ethernet {g}")
        baux = re.findall(r"DUO WAIT sid=(48|56) did=\d+ instance=(\d+)",log)
        exiger(len({pid for sid,pid in baux if sid=="48"})==1 and all(int(pid) in racines and
               (sid!="56" or pid==session[1]) for sid,pid in baux), "Titulaire des reçus")
        exiger(("acteurs=[Exit(9), Exit(0), Exit(0)]" if mort else "acteurs=[Exit(0), Exit(0), Exit(0)]") in log,
               "Sortie des acteurs Ethernet")
        ram = resources(log,"nvme_apres_retrait")
        exiger(ram["dma"]==8+retenus and all(ram[k]==0 for k in ("ipc","processus","utilisateur","upile","meta_actifs")),
               f"Ressources génération {g}")
        hz = int(re.search(r"NET Q35 L2 GSI=23 .*? hz=(\d+)", log)[1])
        mesures = [(op,int(cid),int(t)*1e6/hz) for op,cid,t in
                   re.findall(r"E3D CQE op=(READ|WRITE|FLUSH) cid=(\d+) ticks=(\d+)",log)]
        exiger([(op,cid) for op,cid,_ in mesures]==[(o["op"],o["cid"]) for o in io], "Correspondance des mesures de service")
        lignes.append(dict(generation=g,ram=ram,racines=racines,ordre=ordre,
                           latences_us=mesures,operations=len(io)))
    return dict(cycles=lignes,p9=final,cadres_nic_retenus=retenus,processus_distincts=len(pids))
