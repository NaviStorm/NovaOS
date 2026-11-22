"""Borne ABI des attentes explicites produites par le raccord temporel QEMU."""
import ast
from pathlib import Path
import re
import unittest

PROFILE = Path(__file__).resolve().parents[1] / "service/administration_live_ticket_profile.py"

def validate(source):
    waits = []
    for node in ast.walk(ast.parse(source)):
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            for value in re.findall(r"rt::receive_for\(rt::config\(\d+\),([^)]*)\)", node.value):
                if not value.strip().isdigit():
                    raise ValueError("attente non littérale : vérifier explicitement sa borne")
                ticks = int(value)
                if not 0 < ticks <= 15000:
                    raise ValueError("attente hors borne ABI")
                waits.append(ticks)
    if not waits:
        raise ValueError("aucune attente contrôlée")
    return waits

class WaitTests(unittest.TestCase):
    def test_profile(self):
        self.assertIn(15000, validate(PROFILE.read_text()))

    def test_previous_bug_rejected(self):
        with self.assertRaises(ValueError):
            validate(PROFILE.read_text().replace("config(17),15000", "config(17),25000"))

    def test_overflow_rejected(self):
        with self.assertRaises(ValueError):
            validate(PROFILE.read_text().replace("config(17),15000", "config(17),15001"))

    def test_dynamic_wait_requires_review(self):
        with self.assertRaises(ValueError):
            validate(PROFILE.read_text().replace("config(17),15000", "config(17),deadline"))

if __name__ == "__main__":
    unittest.main()
