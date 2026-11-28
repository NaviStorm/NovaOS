import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from journaux import SupportPrive, history, publish, report
from support import ESP, LOG, MIN_MIO, MIO, S, build, verify
from verifier5n import fat_files, geometry

OBS = {"mode_banc": "actif", "inventaire_classes_pci": [262, 3075]}


class SupportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        cls.src = cls.root / "source.img"
        build(cls.src)
        cls.raw = cls.src.read_bytes()

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_reproduction_et_sortie_exclusive(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "distribution.img"
            build(p)
            self.assertEqual(p.read_bytes(), self.raw)
            with self.assertRaises(FileExistsError):
                build(p)

    def test_deux_crcs_et_FAT_miroir(self):
        for pos in [S + 16, 2 * S + 11, len(self.raw) - S + 16, (LOG + 32) * S]:
            with self.subTest(pos=pos):
                disk = io.BytesIO(self.raw)
                disk.seek(pos)
                disk.write(b"!")
                with self.assertRaises(ValueError):
                    verify(disk)

    def test_nominal_chaine_repertoire(self):
        parts = geometry(self.raw)
        self.assertEqual(len(fat_files(self.raw, parts["NOVALOGS"], "NOVALOGS")), 24)
        self.assertEqual(
            set(fat_files(self.raw, parts["NOVAOS"], "NOVAOS")),
            {"EFI/BOOT/BOOTX64.EFI", "LICENCES.TXT"},
        )

    def test_capacite_non_entiere_en_mio(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "exact.img"
            total = MIN_MIO * MIO // S + 17
            build(p, secteurs=total)
            self.assertEqual(p.stat().st_size, total * S)
            with p.open("rb") as f:
                verify(f)

    def test_dd_sur_plus_grand_et_secteur_etranger(self):
        disk = io.BytesIO(self.raw + bytes(S))
        with self.assertRaisesRegex(ValueError, "GPT secondaire"):
            verify(disk)
        verify(disk, allow_tail=True)
        disk.seek(-1, 2)
        disk.write(b"!")
        with self.assertRaisesRegex(ValueError, "Zone réservée"):
            verify(disk, allow_tail=True)

    def test_format_des_donnees_exportees(self):
        r = report(dict(OBS, serie="PIEGE", epoch="PIEGE"), "0123456789abcdef")
        self.assertNotIn("PIEGE", json.dumps(r))
        self.assertIsNone(r["date_utc"])
        self.assertEqual(r["secure_boot_diagnostic"], "indéterminé")
        self.assertEqual(
            report(OBS, "0" * 16, "2026-01-01T00:00:00+00:00")["date_utc"],
            "2026-01-01T00:00:00+00:00",
        )

    def test_alias_du_fichier_detenu(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)
            owner = SupportPrive(p / "isolé", self.src, len(self.raw))
            try:
                os.link(owner.path, p / "alias")
                with self.assertRaisesRegex(ValueError, "Propriété"):
                    owner.reader()
            finally:
                owner.close()

    def test_echec_entre_synchronisation_et_sceau(self):
        f = io.BytesIO(self.raw)
        areas = verify(f)
        before = history(f, areas)
        r = report(OBS, "0" * 16)
        with patch("journaux.sync"):
            with self.assertRaisesRegex(OSError, "Interruption"):
                publish(f, before, r, fault=2)
            self.assertEqual(history(f, areas)[0][0], "interrompu")
            with self.assertRaisesRegex(ValueError, "modifiés"):
                publish(f, before, r)
            states = history(f, areas)
            slot = publish(f, states, report(OBS, "1" * 16))
            self.assertEqual(slot, "BOOT0002")
            self.assertEqual(states[0], history(f, areas)[0])

    def test_echec_identite_ESP_et_ancienne_somme(self):
        disk = io.BytesIO(self.raw)
        areas = verify(disk)
        disk.seek(areas[0][2][0])
        disk.write(b"X")
        with self.assertRaisesRegex(ValueError, "Sceau"):
            history(disk, areas)
        disk.seek(ESP * S + 71)
        disk.write(b"NOVAUX")
        with self.assertRaisesRegex(ValueError, "GPT/FAT32"):
            verify(disk)
