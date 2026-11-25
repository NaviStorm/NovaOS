"""Pair Ethernet réservé à un socketpair Unix anonyme du moteur de qualification."""
import errno
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
        self.erreur = None
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

    def echanger(self):
        try:
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
                reponses.extend(len(retour).to_bytes(4, "big") + retour)
                self.trames.append(dict(sequence=i, horloge_hote_ns=time.monotonic_ns(),
                                        octets=n, tx_sha256=hashlib.sha256(obs).hexdigest(),
                                        rx_sha256=hashlib.sha256(retour).hexdigest()))
            self.sock.sendall(reponses)
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
        bilan = dict(trames=self.trames, erreur=self.erreur,
                     succes=len(self.trames)==16 and not self.thread.is_alive() and self.erreur is None)
        (self.sortie / "pair.json").write_text(json.dumps(bilan, indent=2) + "\n")
