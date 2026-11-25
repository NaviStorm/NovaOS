"""Pair Ethernet réservé à un socketpair Unix anonyme du moteur de qualification."""
from functools import lru_cache
import errno
import os
import hashlib
import json
import socket
import threading
import time

NIC = bytes.fromhex("525400123456")
PAIR = bytes.fromhex("024e56000001")
TYPE = bytes.fromhex("88b5")


def trame(numero, retour):
    n = 60 if numero % 2 == 0 else 1514
    charge = bytes(((29 * numero + 17 * i) & 255) ^ (165 if retour else 0)
                   for i in range(n - 15))
    return (NIC + PAIR if retour else PAIR + NIC) + TYPE + bytes([numero]) + charge


@lru_cache(maxsize=512)
def motif(numero_bas, retour):
    return trame(numero_bas, retour)[18:]


def trame_longue(numero, retour):
    adresses = NIC + PAIR if retour else PAIR + NIC
    return adresses + TYPE + numero.to_bytes(4, "big") + motif(numero % 256, retour)


def lire(sock, n):
    data = bytearray()
    while len(data) < n:
        bloc = sock.recv(n - len(data))
        if not bloc:
            raise EOFError("Fin du pair avant la trame complète")
        data.extend(bloc)
    return bytes(data)


class Pair:
    def __init__(self, sortie):
        self.sortie = sortie
        self.sock, self.modele = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
        for endpoint in (self.sock, self.modele):
            for option in (socket.SO_SNDBUF, socket.SO_RCVBUF):
                endpoint.setsockopt(socket.SOL_SOCKET, option, 32768)
        self.sock.settimeout(90)
        self.trames = []
        self.lots = {"rearmement":3, "endurance":6250}.get(os.environ.get("NOVAOS_NET_SERIE"),1)
        self.tx_comptes = 0
        self.empreinte_tx = hashlib.sha256()
        self.empreinte_rx = hashlib.sha256()
        self.erreur = None
        self.perte = os.environ.get('NOVAOS_NET_PAIR_PERTE') == 'derniere-reponse'
        self.envoyees = 0
        self.stockage = False
        self.thread = threading.Thread(target=self.echanger, daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def connecter(self, args):
        fd = self.modele.fileno()
        # Un seul pair. Le hub intermédiaire pourrait perdre une trame lors de la
        # contre-pression du socket et confondre ce cas avec une perte dans le NIC.
        pos = next(i for i,v in enumerate(args) if v in ("hubport,id=l2,hubid=17","hubport,id=prive,hubid=17"))
        identifiant = args[pos].split(",")[1]
        args[pos] = f"socket,{identifiant},fd={fd}"
        return (fd,)

    def serie(self):
        for batch in range(self.lots):
            reponses = bytearray()
            for slot in range(16):
                numero = batch * 16 + slot
                attendu = trame_longue(numero, False)
                n = int.from_bytes(lire(self.sock, 4), "big")
                if n != len(attendu):
                    raise ValueError(f"Longueur TX de la trame {numero}: {n}")
                recu = lire(self.sock, n)
                if recu != attendu:
                    raise ValueError(f"Contenu ou séquence TX de la trame {numero}")
                self.empreinte_tx.update(recu)
                self.tx_comptes += 1
                retour = trame_longue(numero, True)
                self.empreinte_rx.update(retour)
                reponses += len(retour).to_bytes(4,"big") + retour
            self.sock.sendall(reponses)
            self.envoyees += 16

    def echanger(self):
        try:
            if self.lots > 1:
                self.serie()
                return
            reponses = bytearray()
            for i in range(16):
                n = int.from_bytes(lire(self.sock, 4), "big")
                if n not in (60, 1514):
                    raise ValueError(f"Longueur Ethernet inattendue {n}")
                obs = lire(self.sock, n)
                if obs != trame(i, False):
                    attendu = trame(i, False)
                    premier = next((j for j,(a,b) in enumerate(zip(obs,attendu)) if a!=b), None)
                    (self.sortie / "tx-divergent.bin").write_bytes(obs)
                    raise ValueError(f"TX position={i} longueur={n}/{len(attendu)} premier_ecart={premier} prefixe={obs[:16].hex()}")
                retour = trame(i, True)
                if not (self.perte and i == 15):
                    reponses.extend(len(retour).to_bytes(4, "big") + retour)
                self.trames.append(dict(sequence=i, horloge_hote_ns=time.monotonic_ns(),
                                        octets=n, tx_sha256=hashlib.sha256(obs).hexdigest(),
                                        rx_sha256=hashlib.sha256(retour).hexdigest()))
            if os.environ.get("NOVAOS_NET_PAIR_P9")=="lectures-flush":
                # La dernière réponse réseau demeure en vol durant les opérations disque.
                stop=time.monotonic()+6
                while time.monotonic()<stop:
                    if b"NET ARM flush hz=" in (self.sortie / "serie.log").read_bytes():
                        self.stockage=True
                        break
                    time.sleep(.001)
                if not self.stockage:
                    raise ValueError("FLUSH invité absent")
            self.sock.sendall(reponses)
            self.envoyees = 15 if self.perte else 16
        except (OSError, EOFError, ValueError) as exc:
            self.erreur = str(exc)

    def __exit__(self, *_):
        self.modele.close()
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError as exc:
            if exc.errno != errno.ENOTCONN:
                raise
        self.thread.join(timeout=3)
        self.sock.close()
        bilan = dict(trames=self.trames, erreur=self.erreur, stockage_avant_derniere_reponse=self.stockage, reponses_envoyees=self.envoyees, perte=self.perte,
                     succes=self.envoyees==16 and len(self.trames)==16 and not self.thread.is_alive() and self.erreur is None)
        if self.lots > 1:
            total = self.lots*16
            bilan.update(trames_tx=self.tx_comptes, trames_rx=self.envoyees, lots=self.lots,
                         tx_sha256=self.empreinte_tx.hexdigest(),rx_sha256=self.empreinte_rx.hexdigest(),
                         succes=self.tx_comptes==self.envoyees==total and not self.thread.is_alive() and self.erreur is None)
        (self.sortie / "pair.json").write_text(json.dumps(bilan, indent=2) + "\n")
