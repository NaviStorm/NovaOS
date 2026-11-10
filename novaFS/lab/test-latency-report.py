#!/usr/bin/env python3
"""Régression du rapport : une queue locale n'est pas un p99 global."""
import runpy
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent
bench = runpy.run_path(str(LAB / "measure-performance.py"))
history = runpy.run_path(str(LAB / "historical-p99.py"))


class Quantiles(unittest.TestCase):
    def test_episode_local_et_journal_incomplet(self):
        lines = []
        for s in range(1, 6):
            # Six appels lents de la même série franchissent le p99 local.
            samples = ["12000000" if s == 3 and i < 6 else "1000000" for i in range(512)]
            top = samples[0]
            lines.append("\t".join(["echantillons_ns", "lecture_seq64k", str(s), *samples]))
            lines.append("\t".join(["lecture_seq64k", str(s), "512", "33554432", "0",
                                     "1000000", "1000000", top, top]))
        result = bench["read_distribution"](lines)
        self.assertEqual(result["toutes_requetes"], {"n": 2560, "p50_ns": 1000000,
                         "p95_ns": 1000000, "p99_ns": 1000000, "max_ns": 12000000})
        self.assertEqual(result["pire_p99_par_serie_ns"], 12000000)
        for invalid in (lines[1:], [*lines, lines[0]], [*lines[:-1], lines[-1].replace("1000000", "0")]):
            with self.assertRaises(ValueError):
                bench["read_distribution"](invalid)

    def test_bornes_de_l_archive(self):
        base = LAB.parent / "docs/validation/v0.9.4"
        distrib = history["extract"](base / "performance-P4-aarch64.json")
        self.assertEqual(distrib["bornes_du_p99_global_ns"], [1746000, 2367792])
        self.assertEqual(distrib["pire_p99_par_serie_ns"], 11738875)


if __name__ == "__main__":
    unittest.main()
