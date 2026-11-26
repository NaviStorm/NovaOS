"""Pair Ethernet réservé à un socketpair Unix anonyme du moteur de qualification."""
from functools import lru_cache
import errno
import os
import re
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


def verifier_redemarrages(serie, pair):
    cycles = pair["cycles"]
    instances = re.findall(r"NET (?:ARM|Q35) pilote pid=(\d+) mode=3 racines=1 generation=(\d+)", serie)
    if (len(instances)!=cycles or len({pid for pid,_ in instances})!=cycles
            or [int(g) for _,g in instances]!=pair["generations"]
            or serie.count("NET L2 pilote fin=Exit(0) racines=0 ipc_avant=0 ipc_apres=0")!=cycles):
        raise ValueError("Instances, racines ou générations non confirmées")
    def cadres(phase):
        lignes = re.findall(r"RAM phase="+phase+r" ([^\r\n]+)",serie)
        return [dict((k,int(v)) for k,v in re.findall(r"(\w+)=(\d+)",ligne)) for ligne in lignes]
    avant,apres = cadres("net_avant_pilote"),cadres("net_apres_pilote")
    compteurs=("resident","tables","kpile","upile","ipc","processus","dma","utilisateur")
    if len(avant)!=cycles or len(apres)!=cycles or any(
        a[k]!=b[k] or a[k]!=avant[0][k] for a,b in zip(avant,apres) for k in compteurs
    ):
        raise ValueError("Croissance des ressources entre incarnations")


class Pair:
    def __init__(self, sortie, *, contention=False, panne_nic=False):
        self.sortie = sortie
        self.sock, self.modele = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
        for endpoint in (self.sock, self.modele):
            for option in (socket.SO_SNDBUF, socket.SO_RCVBUF):
                endpoint.setsockopt(socket.SOL_SOCKET, option, 32768)
        self.sock.settimeout(90)
        self.trames = []
        self.lots = {"rearmement":3, "redemarrage":3, "endurance":6250,"stockage":62500,"pression-rx":3}.get(os.environ.get("NOVAOS_NET_SERIE"),1)
        serie=os.environ.get("NOVAOS_NET_SERIE","")
        self.performance=serie.split("-",1)[1] if serie.startswith(("court-","perf-")) else ""
        if serie=="perf-long-rx":
            self.performance="rx"
        self.pression=serie=="pression-rx"
        self.capture_pression=False
        if self.performance:
            if self.performance not in ("tx","rx","duplex","rtt"):
                raise ValueError("Profil de performance inconnu")
            self.lots=6 if serie.startswith("court-") else 62500 if serie=="perf-long-rx" else 31250
        self.fenetres=[]
        self.contention = contention
        self.panne_nic = panne_nic
        self.cycles = 4 if os.environ.get("NOVAOS_NET_SERIE")=="redemarrage" else 1
        self.generations = []
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
        if self.pression:
            args.extend(["-object",f"filter-dump,id=tracepression,netdev={identifiant[3:]},file={self.sortie}/pression.pcap"])
        return (fd,)

    def observer_pression(self,vm):
        if self.pression and not self.capture_pression and self.envoyees>=17:
            from virtio_net_pression import observer
            self.capture_pression=observer(vm,self.sortie)

    def serie(self):
        for batch in range(1 if self.panne_nic else self.lots*self.cycles):
            reponses = bytearray()
            for slot in range(16):
                numero = (batch % self.lots) * 16 + slot
                n = int.from_bytes(lire(self.sock, 4), "big")
                if n != (1514 if numero % 2 else 60):
                    raise ValueError(f"Longueur TX de la trame {numero}: {n}")
                recu = lire(self.sock, n)
                if self.cycles>1:
                    if numero==0:
                        generation=int.from_bytes(recu[14:16], "big")
                        if generation == 0 or (self.generations and generation<=self.generations[-1]):
                            raise ValueError("Génération non renouvelée")
                        self.generations.append(generation)
                    numero += self.generations[-1] * 65536
                if recu != trame_longue(numero, False):
                    raise ValueError(f"Contenu ou séquence TX de la trame {numero}")
                self.empreinte_tx.update(recu)
                self.tx_comptes += 1
                retour = trame_longue(numero, True)
                if not (self.pression and numero==16):
                    self.empreinte_rx.update(retour)
                    reponses += len(retour).to_bytes(4,"big") + retour
            nombre=16
            if self.pression and batch==0:
                retour=trame_longue(16,True)
                self.empreinte_rx.update(retour)
                reponses += len(retour).to_bytes(4,"big")+retour
                nombre=17
            elif self.pression and batch==1:
                nombre=15
            if self.panne_nic:
                deadline=time.monotonic()+3
                while time.monotonic()<deadline:
                    if b"pci_nvme_read " in (self.sortie/"qemu.trace").read_bytes(): break
                    time.sleep(.002)
                else: raise ValueError("Premier READ matériel NVMe absent avant la panne NIC")
            if self.contention and batch==0:
                # Le premier lot TX a réellement atteint le pair. Attendre une E/S
                # NVMe ultérieure dans la même trace avant de rendre ses réponses RX.
                deadline=time.monotonic()+3
                while time.monotonic()<deadline:
                    trace=(self.sortie/"qemu.trace").read_bytes()
                    first=trace.find(b"virtqueue_fill ")
                    if first>=0 and trace.find(b"pci_nvme_io_cmd ",first)>first:
                        break
                    time.sleep(.002)
                else: raise ValueError("Aucune E/S NVMe après le premier lot TX")
            self.sock.sendall(reponses)
            self.envoyees += nombre

    def echanger(self):
        try:
            if self.performance:
                from virtio_net_fenetres import mesurer
                mesurer(self,self.performance)
                return
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
        if self.pression and not self.capture_pression:
            self.erreur="Saturation du RX non observée dans le modèle"
        bilan = dict(trames=self.trames, erreur=self.erreur, stockage_avant_derniere_reponse=self.stockage, reponses_envoyees=self.envoyees, perte=self.perte,
                     succes=self.envoyees==16 and len(self.trames)==16 and not self.thread.is_alive() and self.erreur is None)
        if self.lots > 1:
            total = 16 if self.panne_nic else self.lots*self.cycles*16
            bilan.update(trames_tx=self.tx_comptes, trames_rx=self.envoyees, lots=self.lots, cycles=self.cycles, generations=self.generations,
                         tx_sha256=self.empreinte_tx.hexdigest(),rx_sha256=self.empreinte_rx.hexdigest(),
                         succes=self.tx_comptes==self.envoyees==total and not self.thread.is_alive() and self.erreur is None)
        if self.performance:
            tx=32 if self.performance=="rx" else self.lots*16
            rx=32 if self.performance=="tx" else self.lots*16
            complet = self.tx_comptes==tx and self.envoyees==rx and self.erreur is None and not self.thread.is_alive()
            bilan.update(succes=complet,profil=self.performance,fenetres=self.fenetres)
        (self.sortie / "pair.json").write_text(json.dumps(bilan, indent=2) + "\n")
