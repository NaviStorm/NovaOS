"""Témoin passif de saturation RX. Le filtre est placé dans le backend QEMU du pair privé."""
import struct
from virtio_net_confinement import dump, qmp
from virtio_net_pair import NIC, trame_longue


def observer(vm, sortie):
    with (sortie/"pression.pcap").open("rb") as capture:
        data=capture.read(65536)
    if len(data)<24:
        return False
    if data[:4]!=bytes.fromhex("d4c3b2a1") or struct.unpack_from("<I",data,20)[0]!=1:
        raise ValueError("PCAP Ethernet de qualification invalide")
    pos=24
    entrees=[]
    emissions=[]
    while pos+16<=len(data):
        taille,original=struct.unpack_from("<II",data,pos+8)
        if taille!=original or not 60<=taille<=1514:
            raise ValueError("Capture tronquée")
        if pos+16+taille>len(data):
            break
        trame=data[pos+16:pos+16+taille]
        retour=trame[:6]==NIC
        file=entrees if retour else emissions
        if trame!=trame_longue(len(file),retour):
            raise ValueError("Séquence Ethernet de saturation distincte")
        file.append(len(file))
        pos+=16+taille
    if len(entrees)!=17 or len(emissions)!=16:
        return False
    etat=qmp(vm,"x-query-virtio-queue-status",path="/machine/peripheral/nic/virtio-backend",queue=0)
    if (etat["vring-num"],etat["inuse"],etat["last-avail-idx"],etat["used-idx"],etat["shadow-avail-idx"])!=(16,0,16,16,16):
        return False
    dump(sortie/"pression-rx.json",dict(trames_entrees=entrees, trames_emises=emissions, file=etat))
    return True
