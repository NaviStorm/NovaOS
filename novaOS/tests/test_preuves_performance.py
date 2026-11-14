"""Vérifier l'inventaire d'une cellule en échec sans chronométrer l'hôte de test."""
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

from integration_main_performances import empreinte, mesurer


class PreuvesPerformance(unittest.TestCase):
    def test_erreur_et_volume_conserves(self):
        with tempfile.TemporaryDirectory() as tmp:
            racine = Path(tmp)
            (racine / "provenance.json").write_text('{"revision_git": "candidat"}')
            sortie = racine / "essai"
            args = SimpleNamespace(archive=racine, sortie=sortie, secteur=512, plateforme="qemu-aarch64",
                                   chemin="groupe", releve="compteurs", cas="neuf-4m", conserver_images=True)

            def defaillance(_):
                (sortie / "volume.raw").write_bytes(b"volume de qualification")
                raise RuntimeError("interruption de la cellule")

            with patch("integration_main_performances.cellule", side_effect=defaillance):
                with self.assertRaisesRegex(RuntimeError, "interruption de la cellule"):
                    mesurer(args)
            bilan = json.loads((sortie / "inventaire.json").read_bytes())
            self.assertIn("interruption de la cellule", bilan["erreur"])
            self.assertEqual(bilan["construction"]["revision_git"], "candidat")
            self.assertEqual(bilan["fichiers"]["volume.raw"]["sha256"], empreinte(sortie / "volume.raw"))
            with self.assertRaises(FileExistsError):
                mesurer(args)


if __name__ == "__main__":
    unittest.main()
