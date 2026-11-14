"""Contrat du banc QoS : série et disque simulés, sans VM ni charge hôte."""
from contextlib import ExitStack, contextmanager, nullcontext
import hashlib
import io
import json
from pathlib import Path
import plistlib
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch
from xml.etree.ElementTree import ParseError

import es_groupees_utm as banc
import qos_utm
from qos_utm_analyse import analyser, phase


class Chronometrie(unittest.TestCase):
    def test_reponse_hmp_complete_et_vcpu_unique(self):
        for lignes, attendu in ((b"* CPU #0: thread_id=12\r\n", 12),
                               (b"* CPU #0: thread_id=12\r\n  CPU #1: thread_id=13\r\n", None),
                               (b"* CPU #1: thread_id=12\r\n", None)):
            with self.subTest(lignes=lignes), patch.object(banc.os, "write", return_value=1):
                console = banc.Console(-1, io.BytesIO())
                console.pending = lignes + b"(qemu) "
                if attendu is None:
                    with self.assertRaises(RuntimeError):
                        banc.thread_moniteur(console)
                else:
                    self.assertEqual(banc.thread_moniteur(console), attendu)

    def test_reponse_terminale_borne_la_sauvegarde(self):
        evenements = []
        c = MagicMock()
        c.attendre.side_effect = lambda s: evenements.append(s.decode())
        def reponse(*_):
            evenements.append("acquittement")
            return "fichier enregistré"
        c.attendre_une.side_effect = reponse
        def horloge():
            evenements.append("horloge")
            return len(evenements)

        with patch.object(banc.os, "write", side_effect=lambda fd, s: evenements.append(s.decode())):
            bornes = banc.publier(c, "source", "copie", None, lambda: evenements.append("HMP"),
                                  horodater=horloge)
        self.assertEqual(evenements, [":", "vi [COMMANDE] source [modifié]", "HMP", "horloge",
                                     "w copie\r", "Saisir OUI puis Entrée : ", "horloge", "OUI\r",
                                     "acquittement", "horloge", "HMP"])
        self.assertEqual(bornes, (4, 7, 10))

    def test_durees_physiquement_impossibles(self):
        trace = dict(debut_trace_ns=0, fin_trace_ns=200, etats_ns={"Running": 100},
                     cpu_par_coeur_ns={"E": 50, "P": 50}, duree_sans_etat_ns=100)
        self.assertEqual(phase(trace)["e_pct"], 50)
        for mauvais in (dict(cpu_par_coeur_ns={"E": -10, "P": 110}),
                        dict(fin_trace_ns=99, duree_sans_etat_ns=-1)):
            with self.subTest(mauvais=mauvais), self.assertRaises(ValueError):
                phase(dict(trace, **mauvais))

    def test_seuil_une_seconde_avant_arrondi(self):
        essais = [c for c in qos_utm.programme() if c["cellule"] == 4 and c["politique"] == "ordinaire"][:2]
        pages = dict.fromkeys(("Pageins", "Pageouts", "Swapins", "Swapouts", "Compressions", "Decompressions"), 0)
        instant = dict(cpu_ticks=[0, 0, 0, 0], taille_page=16384, cout_ns=0, pages=pages,
                       pression=None, disque_mio=0, disque_transferts=0)
        with tempfile.TemporaryDirectory() as temporaire:
            dossier = Path(temporaire)
            for c, duree in zip(essais, (10**9 - 1, 10**9), strict=True):
                charge = dict(profil="repos", ouvriers=[], avant=dict(instant, monotone_ns=0),
                              apres=dict(instant, monotone_ns=duree+4))
                m = dict(cas=c["cas"], secteur=c["secteur"], sonde=False, politique_demandee="ordinaire",
                         qos=dict(charge=charge), environnement="UTM QCOW2", redemarrages=1,
                         octets_relus=True, avant=[1, 2], apres=[2, 3], pic_cadres_session=42,
                         sha256_destination="a"*64, backend_hmp={"flush_operations": 2},
                         image_importee=dict(disponible=True, info=dict(format="qcow2")),
                         commande_monotone_ns=[1, 11, 1+duree], sauvegarde_ms=1000,
                         apres_oui_ms=round((duree-10)/1e6,3))
                (dossier/(qos_utm.nom(c)+".json")).write_text(json.dumps({"essai": c, "mesures": m}))
            [cellule] = analyser(dossier)["cellules"]
            self.assertEqual(cellule["au_moins_1s"], 1)


class Sauvegarde(unittest.TestCase):
    def essai_simule(self, dossier, panne=None, refus=False):
        # L'orchestrateur UTM et publier() restent réels. Les extrémités
        # (UTM, HMP, P9 et Instruments) n'ouvrent aucun périphérique.
        with ExitStack() as pile:
            def remplacer(objet, nom, **args):
                return pile.enter_context(patch.object(objet, nom, **args))
            pile.enter_context(patch.dict(sys.modules, {"cpu_hote_utm": None, "vcpu_darwin": None}))
            import qos_observation as observation
            profil = dossier/"modele.utm"
            (profil/"Data").mkdir(parents=True)
            config = dict(QEMU=dict(AdditionalArguments=["virtio-blk-pci,adresse=test"]),
                          Drive=[{"ImageName": n} for n in ("code.fd","variables.fd","cible.qcow2")],
                          Serial=[])
            (profil/"config.plist").write_bytes(plistlib.dumps(config))
            for nom in ("code.fd", "variables.fd"):
                (profil/"Data"/nom).touch()
            out = dossier/"essai"
            demarrages, reels, bornes = [], [], iter((10, 200010, 1000010))
            simple = MagicMock(fd=1)
            simple.attendre_une.return_value = "capacité révoquée" if refus else "fichier enregistré"

            @contextmanager
            def demarrer(machine, journal):
                demarrages.append(journal.name)
                journal.write_text("BLOC memoire pic_cadres=42\n")
                yield simple

            def sortie(cmd, **_):
                if cmd[0] == "qemu-img":
                    return '{"format":"qcow2"}'
                if cmd[0] == "/usr/bin/pgrep":
                    return "111"
                if cmd[0] == dossier/"generateur":
                    return '{"blocs":4}'
                self.fail(f"Processus hôte non simulé : {cmd}")

            def relire(archive, cible, secteur, ids, blocs, attendu, destination):
                reels.append((archive.name, attendu, destination))
                return [2, 3]

            trace = MagicMock()
            trace.__enter__.return_value = trace
            trace.extraire.return_value = {"test": True}
            if panne == "debut":
                trace.__enter__.side_effect = RuntimeError("Échec au démarrage de la trace")
            elif panne == "arret":
                trace.__exit__.side_effect = RuntimeError("Échec à l’arrêt de la trace")
            elif panne == "export":
                trace.extraire.side_effect = ParseError("XML de trace tronqué")

            remplacer(banc.subprocess, "check_output", side_effect=sortie)
            remplacer(banc.subprocess, "run", side_effect=AssertionError("Sous-processus non prévu"))
            remplacer(banc.os, "write", return_value=1)
            remplacer(banc, "enregistrer", return_value=nullcontext("uuid-test"))
            remplacer(banc, "nommer", side_effect=lambda p, _: (p, "uuid-test"))
            remplacer(banc, "demarrer", side_effect=demarrer)
            remplacer(banc, "image", return_value=b"identites")
            remplacer(banc, "verifier", return_value=[1, 2])
            remplacer(banc, "convertir")
            remplacer(banc, "exporter")
            remplacer(banc, "relire_export", side_effect=relire)
            remplacer(banc, "allocation_importee",
                      return_value={"disponible": True, "info": {"format": "qcow2"}})
            remplacer(banc, "transfert")
            remplacer(banc, "reouvrir_vi")
            remplacer(banc, "operation", return_value=b"contenu entier")
            remplacer(banc, "ouvrir_moniteur", return_value=object())
            remplacer(banc, "thread_moniteur", return_value=333)
            remplacer(banc, "observer_disque",
                      side_effect=lambda moniteur, stats: stats.append({"flush_operations": 2*len(stats)}))
            remplacer(qos_utm, "pid_moteur", return_value=222)
            charge = remplacer(qos_utm, "charger",
                               return_value=nullcontext({"profil": "repos", "ouvriers": []}))
            remplacer(observation, "tracer", return_value=trace)
            remplacer(observation, "relever", return_value={})
            def horloge():
                t = next(bornes)
                return [t, t+1000, t]
            remplacer(observation, "horloges", side_effect=horloge)
            def lancer():
                return banc.essai(profil, dossier/"noyau", dossier/"generateur", 4096, out, False,
                                  False, "ajout-4m-multiple", qos=qos_utm.Observation("ordinaire","repos",True))
            if refus:
                with self.assertRaisesRegex(RuntimeError, "capacité révoquée"):
                    lancer()
                self.assertEqual(reels, [])
                return
            mesure = lancer()
            charge.assert_called_once_with("repos", out, pression=False)
            self.assertEqual(demarrages, ["sauvegarde.serial", "reprise.serial"])
            self.assertEqual(reels, [("relu.utm", b"contenu entier", "copie")])
            self.assertEqual(mesure["sha256_destination"], hashlib.sha256(b"contenu entier").hexdigest())
            self.assertEqual(mesure["backend_hmp"]["flush_operations"], 2)
            self.assertEqual(mesure["commande_monotone_ns"], [10, 200010, 1000010])
            cas = dict(cas="ajout-4m-multiple", secteur=4096, sonde=False, charge="repos", politique="ordinaire")
            qos_utm.verifier(mesure, cas)
            if panne:
                self.assertIn("erreur_instruments", mesure["qos"])
                self.assertNotIn("instruments", mesure["qos"])
            else:
                self.assertEqual(mesure["qos"]["instruments"], {"test": True})
                self.assertEqual(trace.extraire.call_args.kwargs["pid_utm"], 111)
            return mesure, cas

    def test_integrite_rejouee_meme_sans_trace(self):
        for panne in (None, "debut", "arret", "export"):
            with self.subTest(panne=panne), tempfile.TemporaryDirectory() as dossier:
                self.essai_simule(Path(dossier), panne)

    def test_refus_invite_non_masque_par_erreur_arret_trace(self):
        with tempfile.TemporaryDirectory() as dossier:
            self.essai_simule(Path(dossier), "arret", refus=True)

    def test_fenetre_apres_oui_verifiee(self):
        with tempfile.TemporaryDirectory() as dossier:
            m, c = self.essai_simule(Path(dossier))
        m["apres_oui_ms"] = m["sauvegarde_ms"]
        with self.assertRaises(ValueError):
            qos_utm.verifier(m, c)


if __name__ == "__main__":
    unittest.main()
