"""Qualification hôte sur les VM UTM jetables du banc de publication."""
import argparse
import fcntl
import hashlib
import json
import subprocess
from contextlib import contextmanager, nullcontext
from datetime import datetime, timezone
from itertools import permutations
from pathlib import Path

from charge_hote import charger
from es_groupees_utm import essai, publier
from nsh_utm import pid_moteur

POLITIQUES = ("ordinaire", "-b", "-B")
GRAND = "ajout-4m-multiple"
# Scénario, secteur, charge, trace Instruments, compteurs invités, répétitions.
CELLULES = (
    (GRAND, 4096, "stockage", False, False, 8),
    (GRAND, 4096, "stockage", True, False, 8),
    (GRAND, 4096, "cpu", True, False, 6),
    (GRAND, 4096, "repos", False, False, 3),
    ("neuf-4m", 512, "repos", True, True, 3),
    (GRAND, 4096, "stockage", True, True, 3),
)


def programme(repos_trace=False):
    ordres = tuple(permutations(POLITIQUES))
    for rep in range(max(cellule[-1] for cellule in CELLULES)):
        for j in range(len(CELLULES)):
            i = (j + rep) % len(CELLULES)
            cas, secteur, charge, trace, sonde, repetitions = CELLULES[i]
            if rep < repetitions:
                for politique in ordres[(i + rep) % len(ordres)]:
                    yield dict(cellule=i+1, repetition=rep+1, cas=cas, secteur=secteur, charge=charge,
                               trace=trace, sonde=sonde, politique=politique)
    if repos_trace:
        for i, ordre in enumerate(ordres, 1):
            for politique in ordre:
                yield dict(cellule=7, repetition=i, cas=GRAND, secteur=4096, charge="repos",
                           trace=True, sonde=False, politique=politique)


def nom(c):
    noms = {"ordinaire": "ordinaire", "-b": "fond", "-B": "sortie"}
    return f"c{c['cellule']}-r{c['repetition']:02d}-{noms[c['politique']]}"


class Observation:
    def __init__(self, politique, charge, instruments):
        if politique not in POLITIQUES:
            raise ValueError("Politique processus inconnue")
        self.politique = politique
        self.charge = charge
        self.instruments = instruments
        self.resultat = {}

    def attacher(self, machine, tid):
        from qos_observation import relever
        self.machine, self.tid = machine, tid
        self.pid = pid_moteur(machine)
        pids = subprocess.check_output(["/usr/bin/pgrep", "-x", "UTM"], text=True).split()
        if len(pids) != 1:
            raise RuntimeError("PID de l’application UTM ambigu")
        self.utm = int(pids[0])
        self.resultat.update(uuid=machine, pid_utm=self.utm, pid_moteur=self.pid,
                             tid_moniteur=tid, politique=self.politique,
                             avant_politique=relever(self.utm, self.pid, tid))
        if self.politique != "ordinaire":
            if pid_moteur(machine) != self.pid:
                raise RuntimeError("Identité du moteur modifiée avant taskpolicy")
            subprocess.run(["/usr/sbin/taskpolicy", self.politique, "-p", str(self.pid)],
                           check=True, timeout=10)
        self.resultat["apres_politique"] = relever(self.utm, self.pid, tid)

    @contextmanager
    def enregistrer_trace(self, capture):
        # Une panne de l'observateur ne doit empêcher ni l'acquittement ni la
        # relecture. Une erreur de publication elle-même remonte à l'appelant.
        try:
            trace = capture.__enter__()
        except Exception as exc:
            self.resultat["erreur_instruments"] = f"Démarrage : {type(exc).__name__} {exc}"
            yield None
            return
        try:
            yield trace
        finally:
            try:
                capture.__exit__(None, None, None)
            except Exception as exc:
                self.resultat["erreur_instruments"] = f"Arrêt : {type(exc).__name__} {exc}"

    def mesurer(self, console, source, destination, observateur_disque, dossier):
        from qos_observation import horloges, relever, tracer
        horodatages = []

        def horodater():
            h = horloges()
            horodatages.append(h)
            return h[2]

        if pid_moteur(self.machine) != self.pid:
            raise RuntimeError("Identité du moteur modifiée avant publication")
        capture = self.enregistrer_trace(tracer(self.pid, dossier)) if self.instruments else nullcontext()
        with capture as trace:
            with charger(self.charge, dossier, pression=False) as charge:
                self.resultat["avant_commande"] = relever(self.utm, self.pid, self.tid)
                bornes = publier(console, source, destination, None, observer=observateur_disque,
                                 horodater=horodater)
                self.resultat["apres_commande"] = relever(self.utm, self.pid, self.tid)
        if any(not t["arrete_par_banc"] or not t["debut_ns"] <= bornes[0] <= bornes[2] <= t["fin_ns"]
               for t in charge["ouvriers"]):
            raise RuntimeError("Charge prescrite incomplète")
        self.resultat.update(horloges=horodatages, charge=charge)
        if trace is not None and "erreur_instruments" not in self.resultat:
            try:
                self.resultat["instruments"] = trace.extraire(self.tid, bornes, horodatages, pid_utm=self.utm)
            except Exception as exc:
                self.resultat["erreur_instruments"] = f"Extraction : {type(exc).__name__} {exc}"
        return bornes


def enregistrer(f, valeur):
    temp = f.with_suffix(".tmp")
    temp.write_text(json.dumps(valeur, ensure_ascii=False, allow_nan=False, separators=(",", ":"))+"\n")
    temp.replace(f)


def verifier(m, c):
    if not (m["cas"] == c["cas"] and m["secteur"] == c["secteur"] and m["sonde"] == c["sonde"]
            and m["politique_demandee"] == c["politique"] and m["qos"]["charge"]["profil"] == c["charge"]
            and m["redemarrages"] == 1 and m["octets_relus"] and m["apres"][-1] > m["avant"][-1]
            and m["backend_hmp"]["flush_operations"] == 2 and m["image_importee"]["disponible"]
            and m["image_importee"]["info"]["format"] == "qcow2"):
        raise ValueError("Profil, barrières ou relecture incohérents")
    debut, oui, fin = m["commande_monotone_ns"]
    if (not debut <= oui < fin or round((fin-debut)/1e6, 3) != m["sauvegarde_ms"]
            or round((fin-oui)/1e6, 3) != m["apres_oui_ms"]):
        raise ValueError("Chronométrage de sauvegarde incohérent")
    if c["sonde"] and (m["compteur"]["total_avec_controles_locaux"] > 64
                       or len(m["compteur"]["flush_physiques_ms"]) != 2
                       or "attente_irq" in m["compteur"]):
        raise ValueError("Mesure IPC, pilote polling ou barrières absents")


def campagne(a):
    a.sortie.mkdir(parents=True, exist_ok=True)
    plans = [dict(cellule=0,repetition=1,cas=GRAND,secteur=4096,charge="repos",trace=True,
                  sonde=True,politique=a.pilote)] if a.pilote else programme(a.repos_trace)
    fichiers = {False:a.elf.resolve(), True:a.elf_sonde.resolve()}
    sha = {k: hashlib.sha256(v.read_bytes()).hexdigest() for k,v in fichiers.items()}
    for c in plans:
        d = a.sortie / nom(c)
        rec = a.sortie / (d.name+".json")
        if rec.exists():
            lu = json.loads(rec.read_bytes())
            if lu["essai"] != c or lu["elf_sha256"] != sha[c["sonde"]]:
                raise ValueError(f"Preuve divergente dans {rec}")
            continue
        if d.exists():
            raise ValueError(f"Essai interrompu présent dans {d}, conserver ses preuves")
        val = dict(essai=c, elf_sha256=sha[c["sonde"]], debut_utc=datetime.now(timezone.utc).isoformat())
        try:
            m = essai(a.profil, fichiers[c["sonde"]], a.generateur.resolve(), c["secteur"],
                      d.resolve(),c["sonde"],False,c["cas"],
                      qos=Observation(c["politique"], c["charge"], c["trace"]))
            val["mesures"] = m
            verifier(m,c)
        except (OSError, EOFError, RuntimeError, ValueError, KeyError, AssertionError, subprocess.SubprocessError) as e:
            val.update(erreur=type(e).__name__, diagnostic=str(e)[-2000:])
        val["fin_utc"] = datetime.now(timezone.utc).isoformat()
        enregistrer(rec,val)
        statut = f"{val['erreur']} {val['diagnostic']}" if "erreur" in val else val["mesures"]["sauvegarde_ms"]
        print(d.name, statut, flush=True)


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    for arg in ("profil","generateur","elf","elf_sonde","sortie"):
        p.add_argument(arg,type=Path)
    mode = p.add_mutually_exclusive_group()
    mode.add_argument("--pilote", choices=POLITIQUES)
    mode.add_argument("--repos-trace", action="store_true",
                      help="ajouter six triples au repos avec capture Instruments après la matrice principale")
    a=p.parse_args()
    a.sortie.mkdir(parents=True, exist_ok=True)
    with (a.sortie/".mesure-utm.lock").open("a") as verrou:
        fcntl.flock(verrou,fcntl.LOCK_EX)
        campagne(a)
