"""Trois fenêtres de dix secondes, précédées de cinq secondes d'échauffement du pair."""
from functools import lru_cache
import hashlib
import math
import time

from virtio_net_pair import NIC, PAIR, TYPE, lire


@lru_cache(maxsize=1024)
def charge(numero_bas, retour, taille):
    return bytes((((numero_bas * 29 + i * 17) & 255) ^ (165 if retour else 0))
                 for i in range(3, taille - 15))


def paquet(numero, retour, taille, nonce=None):
    image = (NIC + PAIR if retour else PAIR + NIC) + TYPE + numero.to_bytes(4,"big") + charge(numero % 256, retour, taille)
    return image if nonce is None else image[:18] + nonce + image[26:]


class Fenetres:
    def __init__(self):
        self.debut = None
        self.fin = None
        self.compte = [[0,0] for _ in range(3)]
        self.rtt = [[] for _ in range(3)]

    def mesure(self, sens, heure, rtt=None):
        if self.debut is None:
            self.debut = heure
        self.fin = heure
        i = (heure - self.debut - 5_000_000_000) // 10_000_000_000
        if 0 <= i < 3:
            self.compte[i][sens] += 1
            if rtt is not None:
                self.rtt[i].append(rtt)

    def bilan(self, taille):
        valeurs = []
        for i,(tx,rx) in enumerate(self.compte):
            samples=sorted(self.rtt[i])
            row=dict(trames_tx=tx,trames_rx=rx,pps_tx=tx/10,pps_rx=rx/10,
                     tx_Mbit_s=tx*taille*8/10e6,rx_Mbit_s=rx*taille*8/10e6,
                     fenetre_complete=self.debut is not None and self.fin-self.debut >= (15+i*10)*1_000_000_000)
            if samples:
                row["rtt_ms"]={str(p):samples[math.ceil(len(samples)*p/100)-1]/1e6 for p in (50,95,99)}
            valeurs.append(row)
        return valeurs


def mesurer(pair, mode):
    """TX désigne toujours l'émission de l'invité. Le pair horodate les deux sens."""
    taille=64 if mode=="rtt" else 1514
    fenetres=Fenetres()
    anciens=[bytes(8)]*16
    departs=[0]*16
    for lot in range(pair.lots):
        milieu=0<lot<pair.lots-1
        tx=not (milieu and mode=="rx")
        rx=not (milieu and mode=="tx")
        nonces=[]
        for i in range(16):
            numero=lot*16+i
            if tx:
                n=int.from_bytes(lire(pair.sock,4),"big")
                if n!=taille:
                    raise ValueError(f"TX {numero}, longueur {n}")
                data=lire(pair.sock,n)
                heure=time.monotonic_ns()
                if data!=paquet(numero,False,taille,anciens[i] if mode=="rtt" else None):
                    raise ValueError(f"TX {numero}, trame ou nonce distinct")
                pair.empreinte_tx.update(data)
                pair.tx_comptes+=1
                fenetres.mesure(0,heure,heure-departs[i] if mode=="rtt" and lot else None)
            # Les réponses RX du duplex peuvent être en vol pendant la soumission TX.
            if rx:
                nonce=(time.monotonic_ns().to_bytes(8,"big") if mode=="rtt" else None)
                retour=paquet(numero,True,taille,nonce)
                pair.sock.sendall(taille.to_bytes(4,"big")+retour)
                heure=time.monotonic_ns()
                nonces.append((nonce,int.from_bytes(nonce,'big') if nonce else heure))
                pair.empreinte_rx.update(retour)
                pair.envoyees+=1
                fenetres.mesure(1,heure)
        if mode=="rtt":
            anciens, departs = zip(*nonces)
    pair.fenetres=fenetres.bilan(taille)
