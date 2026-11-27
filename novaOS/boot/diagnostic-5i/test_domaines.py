import struct
import unittest
from domaines import legacy
from sonde import state, windows, cpu_flatview


class Passif(unittest.TestCase):
    def setUp(self):
        self.unit = {
            "ver": 16,
            "cap": (47 << 16) | (1 << 10) | 6,
            "rtaddr": 4096,
            "gsts": 1 << 31,
        }

        def page(*words):
            return struct.pack("<512Q", *words, *[0] * (512 - len(words)))

        self.page = page
        self.pages = {
            4096: page(8193),
            8192: page(12289, 0x102),
            12288: page(16387),
            16384: page(20483),
            20480: page(24579),
            24576: page(28674),
        }

    def test_feuille_directionnelle(self):
        self.assertEqual(
            legacy(self.unit, self.pages, {28672}),
            {0: {"did": 1, "mappings": [{"iova": 0, "cadre": 28672, "droits": 2}]}},
        )

    def test_refus_sans_provenance(self):
        for pages, data in (
            ({}, set()),
            (self.pages, set()),
            (self.pages, {4096}),
            ({k: v for k, v in self.pages.items() if k != 8192}, {28672}),
        ):
            with self.assertRaises(ValueError):
                legacy(self.unit, pages, data)

    def test_modes_et_capacites(self):
        for k, v in (("rtaddr", 6144), ("gsts", 0), ("cap", 0), ("ver", 32)):
            with self.assertRaises(ValueError):
                legacy(dict(self.unit, **{k: v}), self.pages, {28672})

    def test_cycles_partage_modes_reserves(self):
        for addr, words in (
            (12288, [4099]),
            (12288, [16387 | 128]),
            (8192, [12293, 0x102]),
            (8192, [12289, 0x101]),
            (24576, [28674, 28673]),
            (4096, [8193, 128]),
        ):
            with self.assertRaises(ValueError):
                legacy(self.unit, dict(self.pages) | {addr: self.page(*words)}, {28672})

    def test_bornes_de_copie(self):
        with self.assertRaises(ValueError):
            legacy(self.unit, dict(self.pages) | {24576: bytes(4095)}, {28672})
        with self.assertRaises(ValueError):
            legacy(self.unit, dict(self.pages) | {65536: bytes(4096)}, {28672})

    def test_registres_limites_au_modele_et_cpu_arrete(self):
        self.assertEqual(
            windows("  00001000-00001fff (prio 0, i/o): intel_iommu", "intel_iommu"),
            [(4096, 8191)],
        )
        self.assertEqual(
            windows("  00001000-00001fff (prio 0, ram): pc.ram", "intel_iommu"), []
        )
        with self.assertRaises(ValueError):
            cpu_flatview('FlatView #1\n AS "virtio", root: dma\n')

        class Running:
            def call(self, command):
                self.command = command
                return {"running": True}

        q = Running()
        with self.assertRaises(ValueError):
            state(q)
        self.assertEqual(q.command, "query-status")


if __name__ == "__main__":
    unittest.main()
