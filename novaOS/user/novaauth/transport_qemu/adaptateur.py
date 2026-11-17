"""Export NBD privé pour un invité QEMU. Le relais reçoit seulement les octets de flux."""
import argparse
import errno
import fcntl
import os
from pathlib import Path
import socket
import stat
import struct
import sys

PAGE = 4096
VOLUME = 512 * PAGE
TAILLE = VOLUME + 2 * PAGE
CELLULE = struct.Struct("<4sBBH16sQQ24s")
OUVRIR, DONNEES, INTERROGER, ACQUITTER, FERMER, ROMPU = range(1, 7)
MAX_DONNEES = 512 - CELLULE.size
REQUETE = struct.Struct(">IHHQQI")
REPONSE = struct.Struct(">IIQ")
MAX_IO = 128 * 1024


def lire(sock, taille):
    data = bytearray(taille)
    vue = memoryview(data)
    while vue:
        n = sock.recv_into(vue)
        if not n:
            raise EOFError
        vue = vue[n:]
    return data


class Pages:
    """Un sens d’écriture par page. Les indices acquittent des cellules uniquement."""
    def __init__(self, relais):
        self.relais = relais
        self.relais.setblocking(False)
        self.tx = bytearray(PAGE)
        self.rx = bytearray(PAGE)
        self.incarnation = bytes(16)
        self.numero_tx = self.numero_rx = self.acquitte = 0
        self.envoye = self.longueur = 0
        self.ferme = False

    def fermer(self):
        self.relais.close()
        self.tx[:] = self.rx[:] = bytes(PAGE)
        self.ferme = True

    def progresser(self):
        if self.ferme:
            return
        try:
            if self.envoye < self.longueur:
                part = memoryview(self.tx)[CELLULE.size+self.envoye:CELLULE.size+self.longueur]
                n = self.relais.send(part)
                if n == 0:
                    raise ConnectionError
                self.envoye += n
            if self.numero_rx == self.acquitte and any(self.incarnation):
                n = self.relais.recv_into(memoryview(self.rx)[CELLULE.size:512])
                if n == 0:
                    raise ConnectionError
                if self.numero_rx == (1 << 64) - 1:
                    raise ConnectionError
                self.numero_rx += 1
                self.entete(DONNEES, n)
        except BlockingIOError:
            pass
        except OSError:
            self.fermer()

    def entete(self, typ, n):
        ack = self.numero_tx if self.envoye == self.longueur else self.numero_tx - 1
        CELLULE.pack_into(self.rx, 0, b"NA24", 1, typ, n, self.incarnation, self.numero_rx, ack, bytes(24))

    def emettre(self, data):
        if self.ferme:
            return
        magic, version, typ, n, incarnation, num, ack, reserve = CELLULE.unpack_from(data)
        if (magic != b"NA24" or version != 1 or n > MAX_DONNEES or any(reserve)
                or any(data[CELLULE.size+n:]) or not any(incarnation)):
            self.fermer()
            return
        if num == self.numero_tx and num and data == self.tx[:len(data)]:
            self.progresser()
            return
        if (num != self.numero_tx + 1 or not self.acquitte <= ack <= self.numero_rx
                or (self.numero_tx == 0 and typ != OUVRIR)
                or (self.numero_tx and incarnation != self.incarnation)
                or (typ == OUVRIR and self.numero_tx)
                or typ not in (OUVRIR, DONNEES, INTERROGER, ACQUITTER, FERMER)
                or bool(n) != (typ == DONNEES)):
            self.fermer()
            return
        if self.envoye < self.longueur:
            # L'appelant doit relire RX pour connaître le numéro TX consommé.
            self.progresser()
            return
        self.incarnation = incarnation
        if ack > self.acquitte:
            self.rx[:] = bytes(PAGE)
            self.acquitte = ack
        self.tx[:len(data)] = data
        self.numero_tx = num
        self.envoye, self.longueur = 0, n
        if typ == FERMER:
            self.fermer()
        else:
            self.progresser()

    def recevoir(self, secteur):
        self.progresser()
        if self.ferme or self.numero_rx == self.acquitte:
            self.entete(ROMPU if self.ferme else INTERROGER, 0)
        else:
            n = CELLULE.unpack_from(self.rx)[3]
            self.entete(DONNEES, n)
        return memoryview(self.rx)[:secteur]


def exporter(client, disk, secteur, pages, observer=None):
    # Négociation oldstyle à export unique, parfois nommée « simple handshake ».
    # Aucune option client ne peut sélectionner un autre export.
    client.sendall(struct.pack(">8sQQI124x", b"NBDMAGIC", 0x420281861253, TAILLE, 5))
    while True:
        magic, flags, cmd, cookie, offset, n = REQUETE.unpack(lire(client, REQUETE.size))
        if magic != 0x25609513 or (n > MAX_IO and cmd == 1):
            return
        if cmd == 2:
            return
        data = lire(client, n) if cmd == 1 else b""
        err, retour = 0, b""
        try:
            if flags or n > MAX_IO:
                err = errno.EINVAL
            elif cmd == 3 and not n and not offset:
                os.fsync(disk)
            elif cmd not in (0, 1) or n == 0 or (offset % secteur) or (n % secteur):
                err = errno.EINVAL
            elif offset+n <= VOLUME:
                if cmd == 1:
                    if os.pwrite(disk, data, offset) != n:
                        raise OSError(errno.EIO, "écriture P9 incomplète")
                else:
                    retour = os.pread(disk, n, offset)
                    if len(retour) != n:
                        raise OSError(errno.EIO, "lecture P9 incomplète")
            elif cmd == 1 and offset == VOLUME and n == secteur:
                pages.emettre(data)
            elif cmd == 0 and offset == VOLUME+PAGE and n == secteur:
                retour = pages.recevoir(secteur)
            else:
                err = errno.EINVAL
        except OSError as exc:
            err = exc.errno or errno.EIO
        if observer is not None:
            observer(cmd, offset, n, flags, err)
        client.sendall(REPONSE.pack(0x67446698,err,cookie))
        if not err and retour:
            client.sendall(retour)


def servir(disk, listener, relais, secteur, observer=None):
    st = os.fstat(disk)
    if not stat.S_ISREG(st.st_mode) or st.st_size != VOLUME or secteur not in (512,4096):
        raise ValueError("copie P9 de 512 pages et géométrie explicite requises")
    fcntl.flock(disk, fcntl.LOCK_EX | fcntl.LOCK_NB)
    pages = Pages(relais)
    try:
        client, _ = listener.accept()
        with client:
            try:
                exporter(client,disk,secteur,pages,observer)
            except (EOFError,ConnectionError):
                pass
    finally:
        pages.fermer()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("copie_p9",type=Path)
    parser.add_argument("secteur",type=int,choices=[512,4096])
    parser.add_argument("--ecoute-fd",type=int,required=True)
    parser.add_argument("--relais-fd",type=int,required=True)
    args = parser.parse_args()
    with os.fdopen(os.open(args.copie_p9, os.O_RDWR | os.O_NOFOLLOW),"r+b",buffering=0) as fichier:
        with socket.socket(fileno=args.ecoute_fd) as ecoute, socket.socket(fileno=args.relais_fd) as relais:
            servir(fichier.fileno(),ecoute,relais,args.secteur)
