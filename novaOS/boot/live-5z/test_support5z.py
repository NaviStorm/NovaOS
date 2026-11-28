"""Contrôles du support livré sur copies ordinaires privées. Argument : répertoire de build."""
import mmap
from pathlib import Path
import shutil
import struct
import sys
import tempfile
import unittest

from image import MIO, S, admettre, construire

BUILD = None
SHA = None


class Support5Z(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if BUILD is None or SHA is None:
            raise unittest.SkipTest("Exécuter avec le candidat et son empreinte de publication")

    def test_authenticite_externe_du_premier_efi(self):
        with tempfile.TemporaryDirectory() as temporaire:
            image = Path(temporaire) / "substitution.img"
            shutil.copyfile(BUILD / "novaos-live.img", image)
            pe = (BUILD / "stage/BOOTX64.EFI").read_bytes()
            with image.open("r+b") as f, mmap.mmap(f.fileno(), 0) as m:
                debut = m.find(pe)
                self.assertGreaterEqual(debut, 0)
                self.assertEqual(m.find(pe, debut + 1), -1)
                # L'instruction d'entrée se trouve dans une section PE initialisée.
                nt, = struct.unpack_from("<I", pe, 0x3c)
                count, = struct.unpack_from("<H", pe, nt + 6)
                opt, = struct.unpack_from("<H", pe, nt + 20)
                entry, = struct.unpack_from("<I", pe, nt + 40)
                sections = [struct.unpack_from("<4I", pe, nt + 24 + opt + i * 40 + 8)
                            for i in range(count)]
                pos, = [raw + entry - va for _, va, size, raw in sections if va <= entry < va + size]
                m[debut + pos] ^= 1
            with self.assertRaisesRegex(ValueError, "Empreinte"):
                admettre(image, SHA)

    def test_image_publiee(self):
        admettre(BUILD / "novaos-live.img", SHA)

    def test_gpt_a_capacite_exacte(self):
        with tempfile.TemporaryDirectory() as tmp:
            image = Path(tmp) / "geometrie.img"
            taille = 83 * MIO + 17 * S
            construire(BUILD / "stage", image, octets=taille)
            self.assertEqual(image.stat().st_size, taille)
            with image.open("rb") as f:
                f.seek(-S, 2)
                dernier = f.read(S)
            self.assertEqual(dernier[:8], b"EFI PART")
            self.assertEqual(struct.unpack_from("<Q", dernier, 24)[0], taille // S - 1)


if __name__ == "__main__":
    BUILD = Path(sys.argv.pop(1))
    SHA = sys.argv.pop(1)
    unittest.main()
