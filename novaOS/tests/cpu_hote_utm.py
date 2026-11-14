"""Temps CPU cumulé du moteur d'une VM jetable, sur l'hôte Darwin."""
import ctypes as c
import os


class Usage(c.Structure):
    # rusage_info_v6 de sys/resource.h. La disposition est commune aux deux
    # bornes de mesure, y compris pour les champs que le banc ne publie pas.
    _fields_ = [("uuid", c.c_uint8 * 16)] + [(nom, c.c_uint64) for nom in (
        "utilisateur", "systeme", "pkg_idle_wkups", "interrupt_wkups", "pageins", "wired_size",
        "resident_size", "phys_footprint", "debut", "fin", "child_user", "child_system",
        "child_idle_wkups", "child_interrupt_wkups", "child_pageins", "child_elapsed",
        "diskio_bytesread", "diskio_byteswritten", "qos_default", "qos_maintenance",
        "qos_background", "qos_utility", "qos_legacy", "qos_user_initiated",
        "qos_user_interactive", "billed_system", "serviced_system", "logical_writes",
        "lifetime_max_phys_footprint", "instructions", "cycles", "billed_energy", "serviced_energy",
        "interval_max_phys_footprint", "runnable_time", "flags", "utilisateur_p", "systeme_p",
        "pinstructions", "pcycles", "energy_nj", "penergy_nj", "secure_time", "secure_ptime",
        "neural_footprint", "lifetime_max_neural", "interval_max_neural", "conclave_footprint",
        "page_wait", "page_cache_hits")] + [("reserved", c.c_uint64 * 6)]


class BaseTemps(c.Structure):
    _fields_ = [("numer", c.c_uint32), ("denom", c.c_uint32)]


class CpuMoteur:
    @classmethod
    def pour_utm(cls, machine):
        from nsh_utm import pid_moteur
        return cls(pid_moteur(machine))

    def __init__(self, pid):
        self.pid = pid
        self.lib = c.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        self.lib.proc_pid_rusage.argtypes = [c.c_int, c.c_int, c.POINTER(Usage)]
        self.lib.proc_pid_rusage.restype = c.c_int
        self.base = BaseTemps()
        systeme = c.CDLL("/usr/lib/libSystem.B.dylib")
        if systeme.mach_timebase_info(c.byref(self.base)) != 0 or not self.base.denom:
            raise RuntimeError("Base de temps Mach indisponible")

    def relever(self):
        compte = Usage()
        if self.lib.proc_pid_rusage(self.pid, 6, c.byref(compte)) != 0:
            erreur = c.get_errno()
            raise OSError(erreur, os.strerror(erreur))
        return compte

    def difference(self, avant, apres):
        if avant.debut != apres.debut or bytes(avant.uuid) != bytes(apres.uuid) or apres.fin:
            raise RuntimeError("Le processus mesuré a changé")
        r = {}
        for nom in ("utilisateur", "systeme", "utilisateur_p", "systeme_p"):
            ticks = getattr(apres, nom) - getattr(avant, nom)
            if ticks < 0:
                raise ValueError("Compteur CPU non monotone")
            r[nom + "_ns"] = ticks * self.base.numer // self.base.denom
        for nom in ("instructions", "cycles"):
            r[nom] = getattr(apres, nom) - getattr(avant, nom)
            if r[nom] < 0:
                raise ValueError("Compteur d’activité non monotone")
        return r
