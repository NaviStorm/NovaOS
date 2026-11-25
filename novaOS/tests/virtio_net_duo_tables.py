"""Relit physiquement la file QI, les reçus et toutes les entrées des deux arbres VT-d."""
import re
import struct
from virtio_net_confinement import reg


def verifier(vm, qt, serial, final, mode):
    if b"NET DUO etat=Ok(())" not in serial:
        raise ValueError("Issue du superviseur: " + serial[-1000:].decode(errors="replace"))
    matches = re.findall(rb"DUO tables root=(0x[0-9a-f]+) qi=(0x[0-9a-f]+) wait=(0x[0-9a-f]+) iqt=(\d+) cookie=(\d+) nic=(true|false) nvme=(true|false) en_attente=(true|false)", serial)
    if len(matches) != 2 or matches[0][:3] != matches[1][:3] or matches[0][3:] != (b"144",b"3",b"true",b"true",b"false"):
        raise ValueError("Points de contrôle du superviseur manquants")
    root, qi, wait = [int(x,16) for x in matches[-1][:3]]
    base = 0xfed90000
    def reg64(address):
        return reg(vm,address) | reg(vm,address+4)<<32
    counters = {offset:reg64(base+offset) for offset in (0x20,0x80,0x88,0x90)}
    device_order = [0,48,56]
    if mode in ("retrait-nic","perte-wait","total-nic"):
        device_order += [56]
    if mode in ("retrait-disque","total-disque","total-nic"):
        device_order += [48]
    if mode == "total-disque":
        device_order += [56]
    cookie = len(device_order)
    if (final["vtd_gsts"] != 0xc4000000 or reg(vm,base+0x34) != 0 or
        counters != {0x20:root,0x80:cookie*48,0x88:cookie*48,0x90:qi} or reg(vm,wait)!=cookie):
        raise ValueError("État matériel ou WAIT différent du résultat annoncé")
    pages = set()
    def unique(pa):
        if not 0 < pa <= 256*1024**2-4096 or pa%4096 or pa in pages:
            raise ValueError("Cadre dupliqué ou hors RAM")
        pages.add(pa)
    def read(pa):
        unique(pa)
        qt.write(f"read {pa:#x} 4096\n".encode())
        line = qt.readline().strip()
        if not re.fullmatch(rb"OK 0x[0-9a-fA-F]{8192}",line):
            raise ValueError("Capture de page incomplète")
        return struct.unpack("<512Q",bytes.fromhex(line[5:].decode()))
    descriptors = read(qi)
    for i,sid in enumerate(device_order):
        did = {0:0,48:1,56:3}[sid]
        cc = 0x11 if not sid else (sid<<32)|(did<<16)|0x31
        tlb = 0xd2 if not sid else (did<<16)|0xe2
        if descriptors[i*6:i*6+6]!=(cc,0,tlb,0,((i+1)<<32)|0x25,wait):
            raise ValueError(f"Descripteurs et WAIT groupe {i+1}")
    if any(descriptors[cookie*6:]):
        raise ValueError("Descripteur QI supplémentaire")
    unique(wait)
    def entries(pa):
        return {i:w for i,w in enumerate(read(pa)) if w}
    def sole(table,index,bits):
        if set(table)!={index} or table[index]&4095 != bits:
            raise ValueError("Portée ou permissions intermédiaires invalides")
        return table[index]&~4095
    ctx = entries(sole(entries(root),0,1))
    disk = mode in ("domaines","retrait-nic","perte-wait")
    nic = mode in ("domaines","retrait-disque")
    held = mode=="perte-wait"
    rights = {}
    for sid,did,addr,perms in [(48,1,0x02000000,[1,2,1,2,2,1]),(56,3,0x10000000,[1,1,2,1,1,2]+[2]*8+[1]*8)]:
        if not (disk if sid==48 else nic):
            continue
        pointer=ctx.pop(sid*2,None)
        if pointer is None or pointer&4095 != 1 or ctx.pop(sid*2+1,None) != (did<<8)|2:
            raise ValueError("Contexte du mauvais domaine")
        pa=pointer&~4095
        for shift in (39,30,21):
            pa=sole(entries(pa),(addr>>shift)&511,3)
        leaf=entries(pa)
        if set(leaf)!=set(range(len(perms))):
            raise ValueError("Nombre de feuilles ou IOVA incorrect")
        for i,v in leaf.items():
            if v&4095 != perms[i]: raise ValueError("Permissions DMA incorrectes")
            unique(v&~4095)
        rights[sid]=leaf
    if ctx: raise ValueError("Autre traduction dans la table de contextes")
    counts=re.findall(rb"DUO epingles avant=(\d+) apres=(\d+) BME_nvme=0 BME_nic=0",serial)
    retained=8+6*disk+26*(nic or held)
    if len(counts)!=1 or int(counts[0][1])-int(counts[0][0])!=retained:
        raise ValueError("Restitution des cadres incohérente")
    expected=[(sid,1 if sid==48 else 3,1 if sid==48 else 2,1,n<=3,n,n*48)
              for n,sid in enumerate(device_order,1) if sid and not (held and n==cookie)]
    observed=[(int(s),int(d),int(o),int(g),b==b"true",int(c),int(t)) for s,d,o,g,b,c,t in
              re.findall(rb"DUO WAIT sid=(\d+) did=(\d+) instance=(\d+) generation=(\d+) publication=(true|false) cookie=(\d+) iqt=(\d+)",serial)]
    if observed!=expected or (matches[-1][-1]==b"true")!=held:
        raise ValueError("Association des acquittements aux instances et aux opérations incohérente")
    return dict(feuilles=rights, registres=counters, pages_distinctes_relues=len(pages), cadres_epingles=retained,
                attente_non_acquittee=held, ordre_des_domaines=device_order, recus=observed)
