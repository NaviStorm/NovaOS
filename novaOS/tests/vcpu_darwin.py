"""Échantillonnage en lecture seule du thread TCG d’une VM Darwin à un vCPU."""
import ctypes as c
import errno
import os
import struct
import threading
import time

from cpu_hote_utm import BaseTemps


class InfoThread(c.Structure):
    # sys/proc_info.h, proc_threadinfo. fill_taskthreadinfo convertit les deux
    # durées en nanosecondes (résolution de la structure source en microsecondes).
    _fields_ = [("utilisateur", c.c_uint64), ("systeme", c.c_uint64)]
    _fields_ += [(nom, c.c_int32) for nom in (
        "usage", "politique", "etat", "flags", "sommeil", "priorite_effective",
        "priorite_base", "priorite_max")]
    _fields_ += [("nom", c.c_char * 64)]


NOMS_TCG = {b"CPU 0/TCG", b"ALL CPUs/TCG"}
ETATS = {1: "executable", 2: "arrete", 3: "attente", 4: "ininterruptible", 5: "suspendu"}


class ThreadsDarwin:
    def __init__(self, pid):
        self.pid = pid
        self.lib = c.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        self.info = self.lib.proc_pidinfo
        self.info.argtypes = [c.c_int, c.c_int, c.c_uint64, c.c_void_p, c.c_int]
        self.info.restype = c.c_int

    def identifiants(self):
        # XNU PROC_PIDLISTTHREADIDS (28) renvoie les ID noyau uniques de 64 bits.
        # PROC_PIDLISTTHREADS (6) renverrait les adresses pthread du client.
        capacite = 64
        while capacite <= 4096:
            tampon = (c.c_uint64 * capacite)()
            n = self.info(self.pid, 28, 0, tampon, c.sizeof(tampon))
            if n <= 0:
                erreur = c.get_errno()
                raise OSError(erreur, os.strerror(erreur))
            if n % 8:
                raise RuntimeError("Liste des identifiants de threads tronquée")
            if n < c.sizeof(tampon):
                return tampon[:n // 8]
            capacite *= 2
        raise RuntimeError("Trop de threads pour identifier sans ambiguïté le vCPU")

    def lire(self, ident):
        t = InfoThread()
        n = self.info(self.pid, 15, ident, c.byref(t), c.sizeof(t))
        if n != c.sizeof(t):
            erreur = c.get_errno()
            raise OSError(erreur, f"Lecture du thread Darwin {ident} ({n} octets): {os.strerror(erreur)}")
        return t

    def trouver(self, tid_hmp):
        ids = self.identifiants()
        # Certaines versions de QEMU donnent le PID pour « thread-id » sur Darwin.
        # Un ID HMP propre au vCPU doit être présent dans le processus contrôlé.
        autorite = tid_hmp is not None and tid_hmp != self.pid
        if autorite and tid_hmp not in ids:
            raise ValueError("Le TID du moniteur QEMU est absent du processus")
        candidats = []
        for ident in ids:
            try:
                t = self.lire(ident)
            except OSError as exc:
                if exc.errno == errno.ESRCH and ident != tid_hmp:
                    continue
                raise
            if t.nom in NOMS_TCG:
                candidats.append(ident)
        if autorite and (candidats == [tid_hmp] or not candidats):
            return tid_hmp, "moniteur et nom" if candidats else "moniteur"
        if not autorite and len(candidats) == 1:
            return candidats[0], "nom du thread TCG"
        raise RuntimeError(f"Identification du vCPU ambiguë (moniteur={tid_hmp}, TCG={candidats})")


class NiveauxCpu:
    """Deux lectures facultatives de PROC_PIDTHREADCOUNTS pour le seul TID mesuré."""

    def __init__(self, threads, ident):
        self.threads, self.ident = threads, ident
        self.bornes = {}
        self.resultat = {}
        systeme = c.CDLL("/usr/lib/libSystem.B.dylib", use_errno=True)
        taille = c.c_size_t()
        def sysctl(nom):
            tampon = c.create_string_buffer(128)
            taille.value = len(tampon)
            if systeme.sysctlbyname(nom.encode(), tampon, c.byref(taille), None, 0):
                erreur = c.get_errno()
                raise OSError(erreur, f"{nom} ({os.strerror(erreur)})")
            return tampon.raw[:taille.value]
        try:
            nombre, = struct.unpack("=I", sysctl("hw.nperflevels"))
            if not 1 <= nombre <= 64:
                raise ValueError("Nombre de niveaux CPU invalide")
            self.noms = [sysctl(f"hw.perflevel{i}.name").rstrip(b"\x00").decode() for i in range(nombre)]
            self.base = BaseTemps()
            if systeme.mach_timebase_info(c.byref(self.base)) or not self.base.numer or not self.base.denom:
                raise ValueError("Base de temps Mach absente")
            self.resultat.update(noms_sysctl=self.noms, numer=self.base.numer, denom=self.base.denom)
        except (OSError, ValueError, struct.error) as exc:
            self._indisponible("configuration", exc)

    def _indisponible(self, borne, exc):
        self.resultat.update(indisponible=borne, errno=getattr(exc, "errno", None), detail=str(exc))

    def relever(self, borne):
        if "indisponible" in self.resultat:
            return
        # bsd/sys/proc_info_private.h : entête de 8 octets puis 5 uint64 par niveau.
        tampon = c.create_string_buffer(8 + len(self.noms)*40)
        try:
            c.set_errno(0)
            debut = time.monotonic_ns()
            n = self.threads.info(self.threads.pid, 34, self.ident, tampon, len(tampon))
            fin = time.monotonic_ns()
            if n <= 0:
                erreur = c.get_errno()
                raise OSError(erreur, f"PROC_PIDTHREADCOUNTS ({os.strerror(erreur)})")
            if n != len(tampon) or struct.unpack_from("=H", tampon)[0] != len(self.noms):
                raise ValueError("Table de compteurs libproc incohérente")
            self.resultat[borne + "_appel_monotone_ns"] = [debut, fin]
            self.bornes[borne] = [struct.unpack_from("=4Q", tampon, 8+i*40) for i in range(len(self.noms))]
            if borne == "dernier":
                niveaux = []
                for nom, a, b in zip(self.noms, self.bornes["premier"], self.bornes["dernier"], strict=True):
                    delta = [y-x for x, y in zip(a, b)]
                    if min(delta) < 0:
                        raise ValueError("Compteur CPU de niveau non monotone")
                    instructions, cycles, ticks_user, ticks_sys = delta
                    u, s = [ticks*self.base.numer//self.base.denom for ticks in (ticks_user, ticks_sys)]
                    niveaux.append({"nom": nom, "utilisateur_ns": u, "systeme_ns": s, "cpu_ns": u+s,
                                    "instructions": instructions, "cycles": cycles})
                self.resultat["deltas"] = niveaux
        except (OSError, ValueError) as exc:
            self._indisponible(borne, exc)


class SondeVcpu:
    """Encadrer une seule publication par demarrer/terminer, en conservant tous les relevés."""

    def __init__(self, pid, tid_hmp=None):
        self.threads = ThreadsDarwin(pid)
        self.ident, self.identification = self.threads.trouver(tid_hmp)
        self.nom = self.threads.lire(self.ident).nom
        self.niveaux = NiveauxCpu(self.threads, self.ident)
        self.periode_ns = 10_000_000
        self.fin = threading.Event()
        self.echantillons = []
        self.ordonnancement = {}
        self.erreur = None
        self.thread = None

    def _relever(self, borne=None):
        a = time.monotonic_ns()
        x = self.threads.lire(self.ident)
        b = time.monotonic_ns()
        if x.nom != self.nom or x.etat not in ETATS:
            raise RuntimeError("Identité ou état Darwin du vCPU inattendu")
        if borne is not None:
            self.ordonnancement[borne] = {
                "priorite_effective": x.priorite_effective, "priorite_base": x.priorite_base,
                "priorite_max": x.priorite_max, "politique": x.politique, "flags": x.flags}
        return ((a + b) // 2, x.utilisateur, x.systeme, x.etat, b - a)

    def _boucle(self):
        try:
            while not self.fin.wait(self.periode_ns / 1e9):
                if len(self.echantillons) >= 32768:
                    raise RuntimeError("Budget d’échantillons du vCPU épuisé")
                self.echantillons.append(self._relever())
        except Exception as exc:
            self.erreur = exc

    def demarrer(self):
        if self.thread is not None:
            raise RuntimeError("La sonde de cette sauvegarde a déjà démarré")
        self.niveaux.relever("premier")
        self.echantillons.append(self._relever("premier"))
        self.thread = threading.Thread(target=self._boucle, name="sonde-vCPU", daemon=True)
        self.thread.start()

    def terminer(self):
        if self.thread is None or self.fin.is_set():
            raise RuntimeError("La sonde ne mesure aucune sauvegarde")
        try:
            dernier = self._relever("dernier")
            self.niveaux.relever("dernier")
        finally:
            self.fin.set()
            self.thread.join()
        if self.erreur:
            raise self.erreur
        # Le dernier appel libproc peut avoir croisé le réveil du préleveur.
        serie = [x for x in self.echantillons if x[0] < dernier[0]]
        serie.append(dernier)
        origine, u0, s0, _, _ = serie[0]
        precedent = serie[0]
        intervalle_max = 0
        for horloge, us, sy, etat, duree in serie[1:]:
            if us < precedent[1] or sy < precedent[2]:
                raise RuntimeError("Compteur CPU du thread non monotone")
            intervalle_max = max(intervalle_max, horloge - precedent[0])
            precedent = (horloge, us, sy, etat, duree)
        return {
            "tid": self.ident, "nom": self.nom.decode(), "identification": self.identification,
            "origine_monotone_ns": origine, "periode_demandee_ns": self.periode_ns,
            "ecoule_ns": dernier[0] - origine,
            "utilisateur_ns": dernier[1] - u0, "systeme_ns": dernier[2] - s0,
            "intervalle_max_ns": intervalle_max,
            "ordonnancement": self.ordonnancement,
            "niveaux_cpu": self.niveaux.resultat,
            "colonnes": ["depuis_origine_ns", "utilisateur_ns", "systeme_ns", "etat", "appel_libproc_ns"],
            "etats": ETATS,
            "echantillons": [[h - origine, u - u0, s - s0, e, d] for h, u, s, e, d in serie],
        }
