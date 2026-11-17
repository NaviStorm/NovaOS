"""Export NBD privé pour un invité QEMU. Le relais reçoit seulement les octets de flux."""
import argparse
import fcntl
import os
from pathlib import Path
import socket
import subprocess

def lire(sock, taille):
    data = bytearray(taille)
    vue = memoryview(data)
    while vue:
        n = sock.recv_into(vue)
        if not n:
            raise EOFError
        vue = vue[n:]
    return data


def servir(disk, listener, relais, secteur, observer=None):
    """Le processus natif possède les deux pages et tout l’état de leur liaison."""
    executable = Path(os.environ["NOVA_ADAPTATEUR"]).resolve(strict=True)
    fcntl.flock(disk, fcntl.LOCK_EX | fcntl.LOCK_NB)
    try:
        client, _ = listener.accept()
        with client:
            fds = disk, client.fileno(), relais.fileno()
            with subprocess.Popen([executable, *map(str, fds), str(secteur)],
                                  pass_fds=fds, stdout=subprocess.PIPE, text=True) as child:
                for line in child.stdout:
                    if observer is not None:
                        observer(*map(int, line.split()))
                if child.wait():
                    raise RuntimeError("échec du transport natif")
    finally:
        relais.close()
        fcntl.flock(disk, fcntl.LOCK_UN)


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
