"""Observer le vCPU UTM avec les outils publics du Mac."""
import ctypes
import datetime
import os
from pathlib import Path
import re
import select
import signal
import struct
import subprocess
import time
import uuid
from xml.etree import ElementTree


def horloges():
    return [time.monotonic_ns(), time.time_ns(), time.monotonic_ns()]


def processus(pid):
    if type(pid) is not int or pid <= 0:
        return dict(pid=pid, indisponible="PID positif requis")
    commande = ["/bin/ps", "-p", str(pid), "-o", "pid=,ppid=,nice=,pri=,state=,time=,comm="]
    try:
        brut = subprocess.run(commande, check=True, capture_output=True, text=True, timeout=3)
        ident, parent, nice, pri, etat, duree, executable = brut.stdout.strip().split(None, 6)
        if int(ident) != pid:
            raise ValueError("PID différent")
        return dict(pid=pid, ppid=int(parent), nice=int(nice), priorite_ps=int(pri),
                    etat_ps=etat, temps_cpu_ps=duree, executable=executable)
    except (OSError, subprocess.SubprocessError, ValueError) as exc:
        return dict(pid=pid, indisponible=str(exc))


def relever(pid_utm, pid_qemu, tid_vcpu):
    debut = time.monotonic_ns()
    return dict(debut_ns=debut, utm=processus(pid_utm), qemu=processus(pid_qemu),
                tid_hmp=tid_vcpu, fin_ns=time.monotonic_ns())


def tables(fichier):
    noeud = ElementTree.parse(fichier).find("node")
    if noeud is None or noeud.find("schema") is None:
        raise ValueError(f"Schéma Instruments absent dans {fichier}")
    colonnes = [c.findtext("mnemonic") for c in noeud.findall("schema/col")]
    defs = {e.get("id"): e for e in noeud.iter() if e.get("id")}
    def concret(e):
        return defs[e.attrib["ref"]] if "ref" in e.attrib else e
    for ligne in noeud.findall("row"):
        champs = dict(zip(colonnes, map(concret, ligne), strict=True))
        processus_xml = champs["process"]
        if processus_xml.tag == "sentinel" or champs["thread"].tag == "sentinel":
            continue
        pid = int(concret(processus_xml.find("pid")).text)
        tid = int(concret(champs["thread"].find("tid")).text)
        yield pid, tid, champs


def fenetre(lignes, a, b):
    """Découper chaque séjour à la fenêtre, sans imputer les lacunes."""
    etats, priorites, coeurs = {}, {}, {}
    fin_precedente = a
    for debut, fin, etat, priorite, coeur in lignes:
        debut, fin = max(debut, a), min(fin, b)
        if debut >= fin:
            continue
        if debut < fin_precedente:
            raise ValueError("Séjours du thread qui se chevauchent")
        fin_precedente = fin
        d = fin - debut
        etats[etat] = etats.get(etat, 0) + d
        if etat == "Running":
            coeurs[coeur] = coeurs.get(coeur, 0) + d
            priorites[priorite] = priorites.get(priorite, 0) + d
    return dict(debut_trace_ns=a, fin_trace_ns=b, etats_ns=etats, cpu_par_coeur_ns=coeurs,
                priorites_running_ns=priorites, duree_sans_etat_ns=b-a-sum(etats.values()))


def qos_sur_intervalle(lignes, a, b):
    dernier = a
    observees = []
    for debut, fin, demandee, effective in lignes:
        debut, fin = max(debut, a), min(fin, b)
        if debut >= fin:
            continue
        if debut < dernier:
            raise ValueError("Intervalles QoS du thread qui se chevauchent")
        observees.append(dict(debut_trace_ns=debut, fin_trace_ns=fin,
                              demandee=demandee, effective=effective))
        dernier = fin
    return dict(intervalles=observees,
                sans_observation_ns=b-a-sum(x["fin_trace_ns"]-x["debut_trace_ns"] for x in observees))


def qos_controleur(pid_utm, threads, a, b):
    releves, intervalles = [], []
    for tid, lignes in sorted(threads.items()):
        # Le nom d’un thread peut changer. Conserver uniquement les noms
        # effectivement vus pendant la fenêtre de commande de ce processus.
        noms = set()
        durees = []
        for debut, fin, demandee, effective, nom in sorted(lignes):
            if fin > a and debut < b:
                noms.add(nom)
                durees.append((debut, fin, demandee, effective))
        qos = qos_sur_intervalle(durees, a, b)
        if qos["intervalles"]:
            intervalles.extend((i["debut_trace_ns"], i["fin_trace_ns"]) for i in qos["intervalles"])
            releves.append(dict(tid=tid, noms_instruments=sorted(noms),
                                couverture_ns=b-a-qos["sans_observation_ns"], **qos))
    couvert, dernier = 0, a
    for debut, fin in sorted(intervalles):
        couvert += max(0, fin-max(dernier, debut))
        dernier = max(dernier, fin)
    return dict(pid=pid_utm, threads=releves, sans_observation_d_aucun_thread_ns=b-a-couvert)


def analyse(dossier, pid, tid_hmp, bornes, ancrages, pid_utm=None):
    if pid_utm is not None and (type(pid_utm) is not int or pid_utm <= 0 or pid_utm == pid):
        raise ValueError("PID du contrôleur UTM invalide")
    run = ElementTree.parse(dossier/"trace-toc.xml").find("run")
    cible = run.find("info/target/process")
    if cible.get("type") != "attached" or int(cible.attrib["pid"]) != pid:
        raise ValueError("La cible de la trace diffère du processus attendu")
    date = run.findtext("info/summary/start-date")
    if not re.search(r"\.\d{3}(?:Z|[+-]\d\d:\d\d)$", date):
        raise ValueError("Précision de la date de trace inconnue")
    debut_utc = datetime.datetime.fromisoformat(date.replace("Z", "+00:00")).astimezone(datetime.timezone.utc)
    delta = debut_utc - datetime.datetime(1970, 1, 1, tzinfo=datetime.timezone.utc)
    origine = ((delta.days*86400+delta.seconds)*1000000 + delta.microseconds)*1000
    if len(bornes) != 3 or len(ancrages) != 3 or not bornes[0] < bornes[1] < bornes[2]:
        raise ValueError("Trois bornes ordonnées attendues")
    offsets = []
    for avant, utc, apres in ancrages:
        if avant > apres:
            raise ValueError("Ancrage d’horloge inversé")
        offsets.append((utc-apres, utc-avant))
    ecart = max(x[1] for x in offsets) - min(x[0] for x in offsets)
    if ecart > 2000000:
        raise ValueError(f"Corrélation des horloges instable de {ecart} ns")
    # Le TOC affiche la date avec trois décimales. Une milliseconde couvre
    # un arrondi ou une troncature à cette précision. L’écart des ancrages
    # couvre l'incertitude de leur lecture et leur dérive observée.
    incertitude = 1000000 + ecart
    t = [b + (i+j)//2-origine for b, (i,j) in zip(bornes, offsets, strict=True)]
    groupes = {}
    for pid_xml, tid, champs in tables(dossier/"trace-etats.xml"):
        if pid_xml != pid:
            continue
        nom = champs["thread"].get("fmt", "")
        if not re.match(r"CPU 0/TCG \(0x[0-9A-Fa-f]+\) \(", nom):
            continue
        debut = int(champs["start"].text)
        fin = debut + int(champs["duration"].text)
        etat = champs["state"].text
        if fin < debut or etat not in ("Running", "Runnable", "Blocked", "Preempted", "Interrupted"):
            raise ValueError("Intervalle ou état de thread inconnu")
        coeur = champs["core"].get("fmt", "")
        typ = re.fullmatch(r"CPU \d+ \(([PE]) Core\)", coeur)
        priorite = champs["priority"].text
        groupes.setdefault(tid, []).append((debut, fin, etat, priorite or "inconnue",
                                             typ.group(1) if typ else "inconnu"))
    if len(groupes) != 1:
        raise ValueError(f"{len(groupes)} threads nommés CPU 0/TCG pour le PID {pid}")
    tid, lignes = next(iter(groupes.items()))
    lignes.sort()
    if not lignes[0][0] <= t[0]-incertitude <= t[2]+incertitude <= lignes[-1][1]:
        raise ValueError("La trace du vCPU ne couvre pas les bornes de la commande")
    resultats = {k: fenetre(lignes,t[i],t[j]) for k,i,j in
                 (("commande",0,2), ("avant_oui",0,1), ("apres_oui",1,2))}
    if not resultats["commande"]["cpu_par_coeur_ns"]:
        raise ValueError("Le vCPU identifié ne présente aucun temps Running")
    lignes_qos, controleur = [], {}
    for pid_xml, tid_xml, champs in tables(dossier/"trace-qos.xml"):
        if (pid_xml, tid_xml) != (pid, tid) and pid_xml != pid_utm:
            continue
        debut = int(champs["start"].text)
        intervalle = (debut, debut+int(champs["duration"].text),
                      champs["requested-qo-s"].get("fmt"), champs["effective-qo-s"].get("fmt"))
        if pid_xml == pid_utm:
            controleur.setdefault(tid_xml, []).append((*intervalle, champs["thread"].get("fmt", "")))
        else:
            lignes_qos.append(intervalle)
    lignes_qos.sort()
    for phase in resultats.values():
        phase["qos"] = qos_sur_intervalle(lignes_qos, phase["debut_trace_ns"], phase["fin_trace_ns"])
    if pid_utm is not None:
        resultats["threads_utm"] = qos_controleur(pid_utm, controleur, t[0], t[2])
    return dict(pid=pid, tid_hmp=tid_hmp, tid_instruments=tid, nom="CPU 0/TCG",
                tid_numerique_identique=tid==tid_hmp, origine_utc=date, ancrages=ancrages,
                bornes_monotones_ns=bornes, incertitude_par_borne_ns=incertitude,
                **resultats)


def exporter_xml(archive, dossier):
    for nom, options in (("toc", ["--toc"]), ("etats", ["--xpath",
            '/trace-toc/run[@number="1"]/data/table[@schema="thread-state"]']), ("qos", ["--xpath",
            '/trace-toc/run[@number="1"]/data/table[@schema="ThreadQoSTable"]'])):
        subprocess.run(["/usr/bin/xcrun", "xctrace", "export", "--input", str(archive),
                        *options, "--output", str(dossier/f"trace-{nom}.xml")],
                       capture_output=True, check=True, timeout=120)


class tracer:
    """Un enregistrement System Trace ciblé. Démarrage et exports hors chronométrage."""
    def __init__(self, pid, dossier):
        self.pid = pid
        if type(pid) is not int or pid <= 0:
            raise ValueError("PID positif requis")
        self.dossier = Path(dossier)
        self.archive = self.dossier / "systeme.trace"
        self.mesures = {}

    def __enter__(self):
        self.dossier.mkdir(exist_ok=True, parents=True)
        if self.archive.exists():
            raise FileExistsError(self.archive)
        nom = f"org.novaos.qualification.{uuid.uuid4().hex}"
        # API Darwin publique de notify.h. notify_cancel libère le descripteur.
        notify = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
        fd, token = ctypes.c_int(-1), ctypes.c_int()
        rc = notify.notify_register_file_descriptor(nom.encode(), ctypes.byref(fd), 0, ctypes.byref(token))
        if rc:
            raise RuntimeError(f"notify_register_file_descriptor a renvoyé {rc}")
        try:
            with (self.dossier/"trace.log").open("x") as log:
                debut = time.monotonic_ns()
                self.proc = subprocess.Popen(["/usr/bin/xcrun", "xctrace", "record", "--template", "System Trace",
                    "--attach", str(self.pid), "--output", str(self.archive),
                    "--notify-tracing-started", nom], stdout=log, stderr=subprocess.STDOUT)
            try:
                while time.monotonic_ns()-debut < 60000000000 and self.proc.poll() is None:
                    if select.select([fd.value], [], [], 0.25)[0]:
                        jeton, = struct.unpack("!I", os.read(fd.value, 4))
                        if jeton != token.value:
                            raise RuntimeError("Jeton de notification de démarrage incohérent")
                        self.mesures["demarrage_ns"] = time.monotonic_ns()-debut
                        return self
                raise RuntimeError(f"System Trace indisponible, voir {self.dossier/'trace.log'}")
            except BaseException:
                self.arreter()
                raise
        finally:
            notify.notify_cancel(token.value)

    def arreter(self):
        if self.proc.poll() is None:
            self.proc.send_signal(signal.SIGINT)
        try:
            return self.proc.wait(timeout=120)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
            raise

    def __exit__(self, exc_type, exc, tb):
        debut = time.monotonic_ns()
        code = self.arreter()
        self.mesures.update(arret_ns=time.monotonic_ns()-debut, sortie_xctrace=code)
        if exc is None and code:
            raise RuntimeError(f"System Trace a quitté avec le code {code}, voir {self.dossier/'trace.log'}")

    def extraire(self, tid_hmp, bornes, horloges, pid_utm=None):
        debut = time.monotonic_ns()
        exporter_xml(self.archive, self.dossier)
        resultat = analyse(self.dossier, self.pid, tid_hmp, bornes, horloges, pid_utm)
        return {**resultat, **self.mesures, "export_analyse_ns": time.monotonic_ns()-debut}
