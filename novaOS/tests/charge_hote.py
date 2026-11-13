"""Charges Darwin bornées, exercées uniquement autour de la sauvegarde de la VM jetable."""
from contextlib import contextmanager, ExitStack
import ctypes
import fcntl
import hashlib
import mmap
import multiprocessing
import os
import re
import subprocess
import tempfile
import time

MIO = 1 << 20


def instantane():
    debut = time.monotonic_ns()
    disque = subprocess.check_output(["iostat", "-Id", "-c", "1", "disk0"], text=True)
    io = disque.splitlines()[-1].split()
    if len(io) != 3 or "disk0" not in disque:
        raise RuntimeError("Statistiques indisponibles pour disk0")
    vm = subprocess.check_output(["vm_stat"], text=True)
    taille = int(re.search(r"page size of (\d+) bytes", vm)[1])
    pages = {k: int(v) for k, v in re.findall(r"^([^:\n]+):\s*(\d+)\.", vm, re.M)}
    pression = int(subprocess.check_output(["sysctl", "-n", "kern.memorystatus_vm_pressure_level"]))
    lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
    ticks = (ctypes.c_uint32 * 4)()
    quantite = ctypes.c_uint32(4)
    if lib.host_statistics(lib.mach_host_self(), 3, ticks, ctypes.byref(quantite)) or quantite.value != 4:
        raise RuntimeError("Compteurs CPU hôte indisponibles")
    fin = time.monotonic_ns()
    return dict(monotone_ns=fin, cout_ns=fin-debut,
                cpu_ticks=list(ticks), charge=os.getloadavg(), pression=pression,
                taille_page=taille, pages=pages, disque_transferts=int(io[1]), disque_mio=float(io[2]))


def travailler(profil, dossier, canal, fin):
    # Un enfant ne survit pas indéfiniment à l'interruption de son banc.
    limite = time.monotonic() + 180
    with ExitStack() as pile:
        if profil == "memoire":
            source = pile.enter_context(mmap.mmap(-1, 256*MIO))
            cible = pile.enter_context(mmap.mmap(-1, 256*MIO))
            for i in range(0, 256*MIO, MIO):
                source[i:i+MIO] = b"a"*MIO
                cible[i:i+MIO] = b"b"*MIO
            src = ctypes.addressof(ctypes.c_char.from_buffer(source))
            dst = ctypes.addressof(ctypes.c_char.from_buffer(cible))
        elif profil == "stockage":
            f = pile.enter_context(tempfile.TemporaryFile(dir=dossier))
            f.truncate(32*MIO)
            fcntl.fcntl(f, fcntl.F_NOCACHE, 1)
            motif = os.urandom(4*MIO)
        elif profil == "cpu":
            motif = b"x"*(256*1024)
        else:
            raise ValueError(profil)
        n = 0
        t0 = time.monotonic_ns()
        cpu = time.process_time_ns()
        while not fin.is_set() and time.monotonic() < limite:
            if profil == "memoire":
                ctypes.memmove(dst, src, 256*MIO)
                src, dst = dst, src
            elif profil == "stockage":
                if os.pwrite(f.fileno(), motif, n % 8 * len(motif)) != len(motif):
                    raise OSError("Écriture temporaire incomplète")
                os.fsync(f.fileno())
            else:
                hashlib.sha256(motif).digest()
            n += 1
            if n == 1:
                canal.send("prêt")
        t1 = time.monotonic_ns()
        canal.send(dict(tours=n, debut_ns=t0, fin_ns=t1, cpu_ns=time.process_time_ns()-cpu,
                        duree_ns=t1-t0, arrete_par_banc=fin.is_set()))
    canal.close()


@contextmanager
def charger(profil, dossier):
    ctx = multiprocessing.get_context("spawn")
    fin = ctx.Event()
    ouvriers = []
    etat = dict(profil=profil, ouvriers=[])
    try:
        for _ in range({"repos": 0, "cpu": min(12, os.cpu_count()), "memoire": 1, "stockage": 1}[profil]):
            recepteur, emetteur = ctx.Pipe(duplex=False)
            p = ctx.Process(target=travailler, args=(profil, dossier, emetteur, fin), daemon=True)
            p.start()
            emetteur.close()
            ouvriers.append((p, recepteur))
        for _, canal in ouvriers:
            if not canal.poll(30) or canal.recv() != "prêt":
                raise RuntimeError("Démarrage de charge incomplet")
        etat["avant"] = instantane()
        yield etat
        etat["apres"] = instantane()
    finally:
        fin.set()
        for p, canal in ouvriers:
            p.join(3)
            if p.is_alive():
                p.terminate()
                p.join()
            try:
                if p.exitcode == 0 and canal.poll():
                    etat["ouvriers"].append(canal.recv())
            finally:
                canal.close()
                p.close()
    if len(etat["ouvriers"]) != len(ouvriers):
        raise RuntimeError("Ouvrier terminé sans compte rendu")
