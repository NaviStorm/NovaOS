"""Tests hors VM du post-traitement public Instruments."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch
from xml.etree import ElementTree as E

from qos_observation import analyse, fenetre, processus, qos_sur_intervalle
from qos_utm_reexport import lire, reexporter


class Lecture(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dossier = Path(tmp.name)
        self.pid, self.tid = 42, 1024
        self.date_ns = 1767225600000000000
        self.bornes = [100000000, 200000000, 300000000]
        self.ancrages = [[t-10, self.date_ns+t, t+10] for t in self.bornes]
        (self.dossier/"trace-toc.xml").write_text(
            '<trace-toc><run><info><target><process type="attached" pid="42"/></target>'
            '<summary><start-date>2026-01-01T00:00:00.000Z</start-date></summary>'
            '</info></run></trace-toc>'
        )
        self.etats = self.table(["start", "thread", "state", "duration", "process", "core", "priority"])
        for debut, duree, etat, coeur in (
            (0, 150000000, "Running", "P"), (150000000, 100000000, "Blocked", None),
            (250000000, 150000000, "Running", "E"),
        ):
            ligne = E.SubElement(self.etats, "row")
            E.SubElement(ligne, "start-time").text = str(debut)
            self.thread(ligne)
            E.SubElement(ligne, "thread-state").text = etat
            E.SubElement(ligne, "duration").text = str(duree)
            self.process(ligne)
            E.SubElement(ligne, "core", fmt=f"CPU 4 ({coeur} Core)" if coeur else "").text = "4"
            E.SubElement(ligne, "sched-priority").text = "31"
        self.qos = self.table(["start", "duration", "process", "thread", "requested-qo-s", "effective-qo-s"])
        self.ecrire()

    def table(self, noms):
        node = E.Element("node")
        s = E.SubElement(node, "schema")
        for nom in noms:
            E.SubElement(E.SubElement(s, "col"), "mnemonic").text = nom
        return node

    def process(self, parent):
        p = E.SubElement(parent, "process")
        E.SubElement(p, "pid").text = str(self.pid)

    def thread(self, parent):
        t = E.SubElement(parent, "thread", fmt=f"CPU 0/TCG (0x{self.tid:x}) (test, pid: {self.pid})")
        E.SubElement(t, "tid").text = str(self.tid)
        self.process(t)

    def ecrire(self):
        for nom, node in (("etats", self.etats), ("qos", self.qos)):
            r = E.Element("trace-query-result")
            r.append(node)
            E.ElementTree(r).write(self.dossier/f"trace-{nom}.xml")

    def lire(self):
        return analyse(self.dossier, 42, 77, self.bornes, self.ancrages)

    def test_decoupage_et_tid_par_nom(self):
        r = self.lire()
        self.assertEqual(r["tid_instruments"], 1024)
        self.assertFalse(r["tid_numerique_identique"])
        self.assertEqual(r["commande"]["cpu_par_coeur_ns"], {"P": 50000000, "E": 50000000})
        self.assertEqual(r["commande"]["duree_sans_etat_ns"], 0)
        self.assertEqual(r["avant_oui"]["etats_ns"], {"Running": 50000000, "Blocked": 50000000})
        self.assertEqual(r["commande"]["qos"]["sans_observation_ns"], 200000000)

    def ajouter_qos(self, debut, duree, demandee, effective):
        r = E.SubElement(self.qos, "row")
        E.SubElement(r, "start-time").text = str(debut)
        E.SubElement(r, "duration").text = str(duree)
        self.process(r)
        self.thread(r)
        E.SubElement(r, "quality-of-service-class", fmt=demandee)
        E.SubElement(r, "quality-of-service-class", fmt=effective)

    def test_qos_par_classe_explicite(self):
        self.ajouter_qos(150000000, 100000000, "Unspecified", "Default")
        self.ecrire()
        qos = self.lire()["commande"]["qos"]
        self.assertEqual(qos["sans_observation_ns"], 100000000)
        self.assertEqual(qos["intervalles"][0]["effective"], "Default")

    def test_threads_du_controleur(self):
        self.pid, self.tid = 57, 7
        self.ajouter_qos(120000000, 120000000, "Default", "Default")
        self.tid = 8
        self.ajouter_qos(210000000, 60000000, "Utility", "User Initiated")
        self.tid = 9
        self.ajouter_qos(1000, 5000, "Default", "Default")  # Avant commande.
        self.pid = 58
        self.ajouter_qos(5000, 444000000, "Background", "Background")  # Tiers.
        self.ecrire()
        r = analyse(self.dossier, 42, 77, self.bornes, self.ancrages, pid_utm=57)
        controleur = r["threads_utm"]
        self.assertEqual([x["tid"] for x in controleur["threads"]], [7, 8])
        self.assertEqual([x["couverture_ns"] for x in controleur["threads"]], [120000000, 60000000])
        self.assertEqual(controleur["sans_observation_d_aucun_thread_ns"], 50000000)
        self.assertEqual(controleur["threads"][1]["intervalles"][0]["effective"], "User Initiated")
        self.assertEqual(r["commande"]["qos"]["intervalles"], [])
        self.assertNotIn("threads_utm", self.lire())

    def test_controleur_inobserve(self):
        r = analyse(self.dossier, 42, 77, self.bornes, self.ancrages, pid_utm=57)
        self.assertEqual(r["threads_utm"], dict(pid=57, threads=[], sans_observation_d_aucun_thread_ns=200000000))
        with self.assertRaisesRegex(ValueError, "PID du contrôleur"):
            analyse(self.dossier, 42, 77, self.bornes, self.ancrages, pid_utm=42)

    def test_tid_ambigu(self):
        self.tid += 1
        r = E.SubElement(self.etats, "row")
        E.SubElement(r, "start-time").text = "0"
        self.thread(r)
        E.SubElement(r, "thread-state").text = "Blocked"
        E.SubElement(r, "duration").text = "400000000"
        self.process(r)
        E.SubElement(r, "sentinel")
        E.SubElement(r, "sentinel")
        self.ecrire()
        with self.assertRaisesRegex(ValueError, "2 threads"):
            self.lire()

    def test_processus_different(self):
        self.pid += 1
        for pid in self.etats.iter("pid"):
            pid.text = str(self.pid)
        self.ecrire()
        with self.assertRaisesRegex(ValueError, "0 threads"):
            self.lire()

    def test_refus_ancrage_insuffisant(self):
        self.bornes[0] = 0
        with self.assertRaisesRegex(ValueError, "couvre"):
            self.lire()

    def test_refus_horloges_instables(self):
        self.ancrages[-1][1] += 3000000
        with self.assertRaisesRegex(ValueError, "instable"):
            self.lire()

    def test_refus_chevauchement_etat(self):
        with self.assertRaisesRegex(ValueError, "chevauchent"):
            fenetre([(0, 80, "Running", "31", "E"), (75, 100, "Running", "31", "P")], 0, 100)

    def test_refus_chevauchement_qos(self):
        with self.assertRaisesRegex(ValueError, "chevauchent"):
            qos_sur_intervalle([(0, 80, "Default", "Default"), (75, 100, "Default", "Default")], 0, 100)

    def test_lacune_conservee(self):
        self.etats.remove(self.etats.findall("row")[1])
        self.ecrire()
        self.assertEqual(self.lire()["commande"]["duree_sans_etat_ns"], 100000000)

    def test_xml_references(self):
        r = self.etats.findall("row")
        r[0][1].set("id", "fil")
        r[1].remove(r[1][1])
        r[1].insert(1, E.Element("thread", ref="fil"))
        self.ecrire()
        self.assertEqual(self.lire()["commande"]["duree_sans_etat_ns"], 0)

    def test_priorite_ps_hors_sonde(self):
        with patch("qos_observation.subprocess.run") as p:
            p.return_value.stdout = "42 1 0 31 S 0:00.04 /Applications/UTM App/UTM\n"
            self.assertEqual(processus(42)["priorite_ps"], 31)
            p.return_value.stdout = "41 1 0 31 S 0:00.04 test"
            self.assertIn("indisponible", processus(42))
        self.assertIn("indisponible", processus(-1))

    def test_reexport_separe(self):
        essai = dict(cellule=2, repetition=6, cas="ajout-4m-multiple", charge="stockage",
                     secteur=4096, sonde=False, trace=True, politique="-b")
        m = dict(cas=essai["cas"], secteur=4096, sonde=False, politique_demandee="-b",
                 qos=dict(charge=dict(profil="stockage"), erreur_instruments="Arrêt : TimeoutExpired",
                          horloges=[[t, self.date_ns+t, t] for t in self.bornes], pid_moteur=self.pid,
                          pid_utm=57, tid_moniteur=77),
                 redemarrages=1, octets_relus=True, apres=[3, 4], avant=[1, 2],
                 backend_hmp=dict(flush_operations=2), image_importee=dict(disponible=True, info=dict(format="qcow2")),
                 commande_monotone_ns=self.bornes, sauvegarde_ms=200, apres_oui_ms=100)
        fiche = self.dossier/"c2-r06-fond.json"
        fiche.write_text(json.dumps(dict(mesures=m, essai=essai)))
        origine = fiche.read_bytes()
        archive = fiche.with_suffix("")/"systeme.trace"
        archive.mkdir(parents=True)
        self.assertIsNone(lire(fiche))
        with patch("qos_utm_reexport.exporter_xml", side_effect=ValueError("Archive incomplète")):
            echec = reexporter(fiche)
        self.assertEqual(lire(fiche)["statut"], "echec")
        def copier(source, sortie):
            self.assertEqual(source, archive)
            for p in self.dossier.glob("trace-*.xml"):
                shutil.copyfile(p, sortie/p.name)
        with patch("qos_utm_reexport.exporter_xml", side_effect=copier) as exporteur:
            r = reexporter(fiche)
            self.assertEqual(lire(fiche), r)
            self.assertEqual(reexporter(fiche), r)
            exporteur.assert_called_once()
        self.assertEqual(r["statut"], "recupere")
        self.assertEqual(fiche.read_bytes(), origine)
        bilan = archive.parent/"post-traitement"
        self.assertEqual(json.loads((bilan/echec["tentative"]/"resultat.json").read_bytes()), echec)
        xml = bilan/r["tentative"]/"trace-etats.xml"
        xml.write_text(xml.read_text()+"\n")
        with self.assertRaisesRegex(ValueError, "relecture"):
            lire(fiche)
        fiche.write_bytes(origine+b"\n")
        with self.assertRaisesRegex(ValueError, "correspond pas"):
            lire(fiche)


if __name__ == "__main__":
    unittest.main()
