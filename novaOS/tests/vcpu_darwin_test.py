"""Étalonner le compteur vCPU Darwin avec un thread et une horloge CPU indépendants."""
import argparse
import ctypes as c
import json
import os
import signal
import subprocess
import sys
import threading
import time
from contextlib import contextmanager
from pathlib import Path

from vcpu_darwin import SondeVcpu, ThreadsDarwin


def charge(avec_pause):
    # Thread nommé uniquement dans ce processus de contrôle.
    nommer = c.CDLL("/usr/lib/libSystem.B.dylib").pthread_setname_np
    nommer.argtypes = [c.c_char_p]
    if nommer(b"CPU 0/TCG") != 0:
        raise RuntimeError("Nom du thread de contrôle refusé")
    print(threading.get_native_id(), flush=True)
    attendre = sys.stdin.readline
    if attendre().strip() != "debut":
        raise RuntimeError("Acquittement de départ attendu")
    debut = time.thread_time_ns()
    if avec_pause:
        print("calcul", flush=True)
    budget = 300_000_000 if avec_pause else 120_000_000
    while time.thread_time_ns() - debut < budget:
        pass
    if not avec_pause:
        time.sleep(0.25)
    print(time.thread_time_ns() - debut, flush=True)
    attendre()


@contextmanager
def temoin(avec_pause):
    mode = "charge-pause" if avec_pause else "charge"
    with subprocess.Popen([sys.executable, "-B", Path(__file__).resolve(), mode],
                          text=True, stdin=subprocess.PIPE, stdout=subprocess.PIPE) as enfant:
        try:
            yield enfant
        finally:
            enfant.stdin.close()
            enfant.wait(timeout=5)


def pause_enfant(enfant, threads, ident):
    assert enfant.stdout.readline().strip() == "calcul"
    enfant.send_signal(signal.SIGSTOP)
    try:
        pid, statut = os.waitpid(enfant.pid, os.WUNTRACED)
        assert pid == enfant.pid and os.WIFSTOPPED(statut) and os.WSTOPSIG(statut) == signal.SIGSTOP
        t0 = time.monotonic_ns()
        x = threads.lire(ident)
        time.sleep(.25)
        y = threads.lire(ident)
        t1 = time.monotonic_ns()
    finally:
        enfant.send_signal(signal.SIGCONT)
    return t0, t1, dict(temps_arrete_ns=t1-t0, cpu_durant_arret_ns=y.utilisateur + y.systeme
                        - x.utilisateur - x.systeme, etats_aux_bornes=[x.etat, y.etat])


def etalonner(avec_pause=False):
    with temoin(avec_pause) as enfant:
        ident = int(enfant.stdout.readline())
        threads = ThreadsDarwin(enfant.pid)
        assert threads.trouver(enfant.pid) == (ident, "nom du thread TCG")
        assert threads.trouver(ident) == (ident, "moniteur et nom")
        try:
            threads.trouver(1 << 63)
        except ValueError:
            pass
        else:
            raise AssertionError("Une identité absente a été acceptée")
        sonde = SondeVcpu(enfant.pid)
        sonde.demarrer()
        enfant.stdin.write("debut\n")
        enfant.stdin.flush()
        if avec_pause:
            t0, t1, pause = pause_enfant(enfant, threads, ident)
        reference = int(enfant.stdout.readline())
        p = sonde.terminer()
        observe = p["utilisateur_ns"] + p["systeme_ns"]
        assert abs(observe - reference) <= 2_000_000, (observe, reference)
        assert p["ecoule_ns"] >= 350_000_000
        assert len(p["colonnes"]) == 5 and all(len(x) == 5 for x in p["echantillons"])
        assert all(x[0] <= y[0] for x, y in zip(p["echantillons"], p["echantillons"][1:]))
        resultat = dict(reference_thread_time_ns=reference, observe_ns=observe, ecoule_ns=p["ecoule_ns"],
                        lectures=len(p["echantillons"]), intervalle_max_ns=p["intervalle_max_ns"])
        niveaux = p["niveaux_cpu"]
        if "deltas" in niveaux:
            assert [x["nom"] for x in niveaux["deltas"]] == niveaux["noms_sysctl"]
            total = sum(x["cpu_ns"] for x in niveaux["deltas"])
            assert abs(total-reference) <= 2_000_000, (total, reference)
        else:
            assert niveaux["indisponible"] and niveaux["detail"]
        resultat["niveaux_cpu"] = niveaux
        if avec_pause:
            origine = p["origine_monotone_ns"]
            pendant = [x for x in p["echantillons"] if t0 < origine + x[0] < t1]
            assert len(pendant) >= 2 and len({x[1] + x[2] for x in pendant}) == 1
            assert pause["cpu_durant_arret_ns"] == 0
            pause.update(lectures=len(pendant), etats_echantillonnes=sorted({x[3] for x in pendant}))
            resultat["pause_confirmee_par_waitpid"] = pause
        else:
            assert any(x[3] == 3 for x in p["echantillons"])
        return resultat


if __name__ == "__main__":
    if sys.argv[1:] in (["charge"], ["charge-pause"]):
        charge(sys.argv[1] == "charge-pause")
    else:
        options = argparse.ArgumentParser(description=__doc__)
        options.add_argument("--pause", action="store_true",
                             help="suspendre 250 ms le seul enfant créé par ce test")
        print(json.dumps(etalonner(options.parse_args().pause), indent=2))
