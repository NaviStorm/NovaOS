"""Vérifier l'inventaire d'une cellule en échec sans chronométrer l'hôte de test."""
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

import cli
from integration_main_performances import empreinte, mesurer


class PreuvesPerformance(unittest.TestCase):
    def test_journal_prive_sans_trace_lourde(self):
        with tempfile.TemporaryDirectory() as tmp:
            racine = Path(tmp)
            (racine / "build").mkdir()
            journal = racine / "sauvegarde.serial"

            def machine(arch, secteur, image, test, *, iso):
                iso.write_bytes(b"media synthetique")
                return ["sh", "-c", "echo fin-du-passage"], {"NOVA_ISO": str(iso)}

            with patch.object(cli, "ROOT", racine), patch.object(cli, "machine", side_effect=machine):
                with cli.Terminal("aarch64", 512, racine / "volume.raw", "test", journal=journal) as vm:
                    self.assertEqual(vm.vm.wait(timeout=5), 0)
            self.assertEqual(journal.read_text().strip(), "fin-du-passage")
            config = json.loads(journal.with_suffix(".vm.json").read_bytes())
            self.assertEqual(config["iso_sha256"], empreinte(journal.with_suffix(".iso")))
            self.assertNotIn("-trace", config["commande"])
            self.assertEqual(config["octets_secteur"], 512)

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
