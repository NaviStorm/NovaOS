"""Observe les frontières de lecture et d’écriture des exécutables, hors chronométrage."""
import argparse
import hashlib
import json
import os
import random
import resource
import signal
import socket
import subprocess
import tempfile
from pathlib import Path

def inspect(exe, root):
    data = random.Random(271).randbytes(3079)
    source = root/"source"
    source.write_bytes(data)
    files = [root/f"f{i}" for i in range(3)]
    # Une écriture réussie sur un socket datagramme produit exactement un message.
    reader, writer = socket.socketpair(socket.AF_UNIX, socket.SOCK_DGRAM)
    with reader, writer, source.open("rb") as src:
        reader.settimeout(5)
        p = subprocess.Popen([exe,*files],stdin=src,stdout=writer,stderr=subprocess.PIPE)
        messages = []
        observed = bytearray()
        while len(observed)<len(data):
            chunk = reader.recv(65535)
            messages.append(len(chunk))
            observed.extend(chunk)
        err = p.communicate(timeout=5)[1]
        assert not err and p.returncode==0 and observed==data
    assert all(f.read_bytes()==data for f in files)
    # Deux O_APPEND sur le même inode rendent visible le découpage sur fichier ordinaire.
    alias = root/"alias"
    alias.write_bytes(b"")
    output = root/"stdout"
    with source.open("rb") as src, output.open("wb") as dst:
        subprocess.run([exe,"-a",alias,alias],stdin=src,stdout=dst,capture_output=False,
                       stderr=subprocess.PIPE,check=True)
    expected = bytearray()
    offset = 0
    for n in messages:
        expected.extend(data[offset:offset+n]*2)
        offset += n
    assert alias.read_bytes()==expected and output.read_bytes()==data
    # L'écriture refusée arrête le processus après sa première lecture. Le parent
    # observe l'offset de la même description de fichier, sans tracer le processus.
    with source.open("rb") as src, output.open("rb") as read_only:
        res = subprocess.run([exe,"--output-error=exit",*files],stdin=src,
                             stdout=read_only,stderr=subprocess.PIPE)
        first_read = src.tell()
    assert res.returncode==1 and res.stderr and first_read==messages[0]
    # Le second tour atteint le quota sur stdout. Relever chaque préfixe effectif.
    def limit():
        resource.setrlimit(resource.RLIMIT_FSIZE,(1537,1537))
        signal.signal(signal.SIGXFSZ,signal.SIG_IGN)
    with source.open("rb") as src, output.open("wb") as dst:
        res = subprocess.run([exe,"--output-error=exit",*files],stdin=src,stdout=dst,
                             stderr=subprocess.PIPE,preexec_fn=limit)
    assert res.returncode==1 and res.stderr
    sizes=[f.stat().st_size for f in [output,*files]]
    assert 0<sizes[0]<len(data) and all(f.read_bytes()==data[:n] for f,n in zip([output,*files],sizes))
    return dict(binaire=str(exe),sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),
                ecritures_stdout=messages,deux_fichiers_alias_octets=len(expected),
                premiere_lecture=first_read,statut_quota=res.returncode,
                ecritures_avant_arret=sizes)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie",type=Path)
    p.add_argument("binaires",nargs="+",type=Path)
    args=p.parse_args()
    with tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"]) as tmp:
        result=[inspect(exe.resolve(),Path(tmp)) for exe in args.binaires]
    assert all(r["ecritures_avant_arret"]==result[0]["ecritures_avant_arret"] for r in result)
    with args.sortie.open("x") as f:
        json.dump(result,f,indent=2)
        f.write("\n")
