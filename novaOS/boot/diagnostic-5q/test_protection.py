"""Frontières des admissions et contre-exemples pour le juge hors VM."""

from pathlib import Path
import copy
import json
import struct
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from politique import PAGE, ROLES, Registre, source_usb
from preuves import BASE, NOVA, cellule, verif_tables, traductions


class Admissions(unittest.TestCase):
    def setUp(self):
        self.epoch = "3a" * 16
        self.reg = Registre(1 << 32, 32, self.epoch)

    def test_epoch(self):
        for e in ("00" * 16, "ab", "zz" * 16, None):
            with self.subTest(epoch=e), self.assertRaises(ValueError):
                Registre(1 << 32, 32, e)

    def test_roles(self):
        for role, length in (
            (0, 1),
            (9, 1),
            (1, 2049),
            (7, 65537),
            (2, 0),
            (True, 4),
            (1, 1.5),
        ):
            with self.subTest(role=role), self.assertRaises(ValueError):
                self.reg.proposer(1, role, length)
        self.assertEqual(self.reg.feuilles(), [])

    def test_independance(self):
        for seq, (role, (_, droits, maximum, _)) in enumerate(ROLES.items(), 1):
            op = self.reg.proposer(seq, role, maximum)
            self.reg.confirmer(op)
            self.assertEqual(op["droits"], droits)
        frames = [f["cadre"] for f in self.reg.feuilles()]
        self.assertEqual(len(frames), len(set(frames)))
        self.assertGreaterEqual(min(frames), self.reg.reserve + 32 * PAGE)
        self.reg.clore(91, 91, 1)
        with self.assertRaises(ValueError):
            self.reg.clore(91, 91, 1)
        with self.assertRaises(ValueError):
            self.reg.proposer(9, 7, 8)

    def test_quota_et_rejeu(self):
        self.reg.confirmer(self.reg.proposer(1, 1, 520))
        for seq, role in ((1, 2), (2, 1)):
            with self.assertRaises(ValueError):
                self.reg.proposer(seq, role, 128)

    def test_contradiction(self):
        wanted = self.reg.proposer(1, 1, 520)
        for violation in ({"droits": 3}, {"gpa": 4096}, {"iova": 0}, {"pages": 3}):
            with self.assertRaises(ValueError):
                self.reg.confirmer(dict(wanted, **violation))
        self.assertEqual(self.reg.loans, [])

    def test_extension_directionnelle(self):
        first = self.reg.proposer(1, 7, PAGE)
        self.reg.confirmer(first)
        self.reg.confirmer(self.reg.proposer(2, 8, PAGE))
        extra = self.reg.proposer(3, 7, 15 * PAGE)
        self.assertEqual(extra["gpa"], first["gpa"] + PAGE)
        self.assertEqual(extra["iova"], first["iova"] + PAGE)
        self.reg.confirmer(extra)
        for role, n in ((7, PAGE), (8, PAGE), (7, 1)):
            with self.assertRaises(ValueError):
                self.reg.proposer(4, role, n)

    def test_quarantaine(self):
        for idx, role in enumerate(ROLES, 1):
            self.reg.confirmer(
                self.reg.proposer(idx, role, PAGE if role in (7, 8) else 16)
            )
        saved = self.reg.feuilles()
        for state in ((10, 9, 1), (10, 10, 2), (0, 0, 1)):
            with self.assertRaises(ValueError):
                self.reg.clore(*state)
            self.assertEqual(self.reg.feuilles(), saved)

    def test_origine(self):
        part = ["partition", 1]
        path = [["acpi", 1, 0], ["pci", 4, 0], ["usb", 1, 0], part]
        self.assertEqual(source_usb(path, 32, part), 1)
        for bad in (
            [],
            [[], *path[1:]],
            path[:2] + [["usb", 0, 0], part],
            path[:2] + [["usb", 1, 1], part],
            path + [part],
        ):
            with self.assertRaises(ValueError):
                source_usb(bad, 32, part)
        with self.assertRaises(ValueError):
            source_usb(path, 40, part)


class Contrepreuves(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with zipfile.ZipFile(BASE / "observations.zip") as z:
            cls.pieces = {
                n.split("/", 1)[1]: z.read(n)
                for n in z.namelist()
                if n.startswith("actif-nominal/")
            }
        cls.key = (NOVA / "boot/diagnostic-5m/autorite-laboratoire.pub").read_bytes()

    def setUp(self):
        self.b = json.loads(self.pieces["bilan.json"])
        self.reg = Registre(self.b["reserve"], self.b["rids"]["xhci"], self.b["epoque"])
        for op in self.b["prets"]:
            self.reg.confirmer(op)

    def test_droits_modifies(self):
        table = bytearray(self.pieces["avant-load.bin"])
        idx = 17 * PAGE + 8 * (self.reg.loans[0]["iova"] // PAGE)
        table[idx] |= 2
        with self.assertRaises(ValueError):
            verif_tables(table, self.b, self.reg)

    def test_cadre_pe(self):
        table = bytearray(self.pieces["avant-load.bin"])
        idx = 17 * PAGE + 8 * (self.reg.loans[0]["iova"] // PAGE)
        struct.pack_into("<Q", table, idx, self.b["avant_start"]["base"] | 2)
        with self.assertRaises(ValueError):
            verif_tables(table, self.b, self.reg)

    def test_traduction_avant_publication(self):
        trace = self.pieces["trace.log"]
        b = copy.deepcopy(self.b)
        b["publications"] = [len(trace)] * len(self.reg.loans)
        with self.assertRaises(ValueError):
            traductions(trace, b, self.reg)

    def test_entree_et_signature(self):
        for nom in (
            "entree.bin",
            "transfert.bin",
            "origine.bin",
            "cpu.log",
            "apres.ppm",
        ):
            altered = bytearray(self.pieces[nom])
            if nom == "cpu.log":
                altered.extend(b"table cpu=0 pc=1000 gpa=100000000 octets=8\n")
            else:
                altered[-1] ^= 1
            values = dict(self.pieces)
            values[nom] = bytes(altered)
            with (
                self.subTest(piece=nom),
                tempfile.TemporaryDirectory() as tmp,
                self.assertRaises(ValueError),
            ):
                cellule("actif", "nominal", values.__getitem__, self.key, Path(tmp))


if __name__ == "__main__":
    unittest.main()
