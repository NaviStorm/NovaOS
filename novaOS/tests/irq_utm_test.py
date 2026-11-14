"""Contrôles hors VM de la relecture des fenêtres de sauvegarde."""
import json
from pathlib import Path
import tempfile
import unittest

from irq_utm import analyser, lire_latences, verifier_fiche


class Relecture(unittest.TestCase):
    def setUp(self):
        temporaire = tempfile.TemporaryDirectory()
        self.addCleanup(temporaire.cleanup)
        self.dossier = Path(temporaire.name)
        self.journal = self.dossier / "sauvegarde.serial"
        self.texte = (
            "SAVE latences_bloc n=4\n"
            "SAVE latences_bloc debut=0 valeurs=[400,801,1202,1602]\n"
            "SAVE phases=[0,0,0] bloc=[1,100,1,200,2,700]\n"
            "SAVE repos=0 flush_physiques=[300,400]\n"
            "BLOC memoire cadres=1 hz=62500000\n"
        )
        self.journal.write_text(self.texte)
        self.compteur = dict(bloc={
            "lecture": dict(nombre=1, millisecondes=0.002),
            "ecriture": dict(nombre=1, millisecondes=0.003),
            "flush": dict(nombre=2, millisecondes=0.011),
        }, total_avec_controles_locaux=34)

    def test_trace_valide(self):
        trace = lire_latences(self.journal, self.compteur)
        self.assertEqual(trace["commandes_ticks_type"], [400, 801, 1202, 1602])
        self.assertEqual(trace["hz"], 62500000)

    def test_anomalies_de_trace(self):
        for avant, apres, motif in (
            ("[400,801", "[404,801", "en ticks"),
            ("1202,1602", "1602,1202", "F1/F2"),
            ("debut=0", "debut=1", "discontinus"),
            ("n=4", "n=5", "incomplète"),
            ("[400,801", "[400.0,801", "mal encodée"),
            ("hz=62500000", "hz=0", "nulle"),
        ):
            with self.subTest(apres=apres):
                self.journal.write_text(self.texte.replace(avant, apres))
                with self.assertRaisesRegex(ValueError, motif):
                    lire_latences(self.journal, self.compteur)

    def test_relecture_fiche(self):
        pages = dict.fromkeys(("Pageins", "Pageouts", "Swapins", "Swapouts",
                               "Compressions", "Decompressions"), 0)
        hote = dict(cpu_ticks=[0, 0, 0, 0], pression=0, pages=pages,
                    disque_transferts=0, disque_mio=0, taille_page=16384, cout_ns=0)
        debut, oui, fin = 1000000000, 1500000000, 2000000000
        ech = [[t, 0, 0, 1, 0] for t in (0, debut, oui, fin, fin+1)]
        mesure = dict(cas="inserer-debut", secteur=512, politique_demandee="ordinaire",
                      environnement="UTM QCOW2", preallocation=False, sonde=True,
                      redemarrages=1, octets_relus=True, avant=[1, 2], apres=[3, 4],
                      sha256_destination="0"*64, backend_hmp=dict(flush_operations=2),
                      image_importee=dict(disponible=True, info=dict(format="qcow2"), octets_absents=4096),
                      commande_monotone_ns=[debut, oui, fin], sauvegarde_ms=1000, apres_oui_ms=500,
                      charge_hote=dict(profil="repos", avant=dict(hote, monotone_ns=0),
                                       apres=dict(hote, monotone_ns=fin+10)),
                      vcpu=dict(origine_monotone_ns=0, utilisateur_ns=0, systeme_ns=0,
                                periode_demandee_ns=10000000, echantillons=ech), compteur=self.compteur)
        fichier = self.dossier / "resultats.json"
        fichier.write_text(json.dumps(mesure))
        essai = dict(cas=mesure["cas"], secteur=512, charge="repos", politique="ordinaire",
                     sonde=True, mode="polling")
        fiche = dict(releves=mesure, analyse=analyser(mesure),
                     requetes=lire_latences(self.journal, self.compteur))
        verifier_fiche(fiche, self.dossier, essai)
        fiche["analyse"]["commande_ns"] -= 1
        with self.assertRaisesRegex(ValueError, "horodatages"):
            verifier_fiche(fiche, self.dossier, essai)
        fiche["analyse"]["commande_ns"] += 1
        fiche["requetes"]["commandes_ticks_type"].reverse()
        with self.assertRaisesRegex(ValueError, "trace bloc archivée"):
            verifier_fiche(fiche, self.dossier, essai)
        fiche["requetes"]["commandes_ticks_type"].reverse()
        fichier.write_text(json.dumps(dict(mesure, sha256_destination="1"*64)))
        with self.assertRaisesRegex(ValueError, "résultat brut"):
            verifier_fiche(fiche, self.dossier, essai)


if __name__ == "__main__":
    unittest.main()
