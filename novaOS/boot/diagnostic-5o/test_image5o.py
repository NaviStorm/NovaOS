import errno
import mmap
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from image5o import (
    MIO,
    S,
    chargeur,
    copier,
    creer,
    etendues,
    lecteur,
    secteurs,
    verifier,
)


class ImageTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.p = Path(self.tmp.name) / "image.img"
        self.pe = chargeur()
        self.n = 68 * MIO + 17 * S
        creer(self.p, self.n, self.pe)

    def test_capacite_non_ronde_et_miroir(self):
        dest = self.p.with_name("copie.img")
        self.assertEqual(
            copier(self.p, dest, self.n, self.pe)["secondaire_lba"], self.n // S - 1
        )
        with self.assertRaises(FileExistsError):
            copier(self.p, dest, self.n, self.pe)

    def test_fat_invalide_libere_les_vues_du_mmap(self):
        from verifier5n import fat_files, geometry

        with lecteur(self.p) as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_COPY) as disk:
            span = geometry(disk)["NOVAOS"]
            disk[span[0] + 11] ^= 1
            with self.assertRaisesRegex(ValueError, "BPB"):
                fat_files(disk, span, "NOVAOS")

    def test_capacite_incompatible(self):
        for n in (0, 68 * MIO - 512, 68 * MIO + 1, (1 << 32) * S):
            with self.subTest(n=n), self.assertRaises(ValueError):
                secteurs(n)
        with lecteur(self.p) as f, self.assertRaises(ValueError):
            verifier(f, self.n + S, self.pe)

    def test_refuse_donnees_etrangeres_et_journal_modifie(self):
        with self.p.open("r+b") as f:
            for off in (S + 16, 3 * MIO, 36 * MIO, self.n - S + 16):
                f.seek(off)
                old = f.read(1)
                f.seek(off)
                f.write(bytes([old[0] ^ 1]))
                f.flush()
                with self.subTest(off=off), self.assertRaises(ValueError):
                    verifier(f, self.n, self.pe)
                f.seek(off)
                f.write(old)
                f.flush()

    def test_sorties_existantes_et_liens(self):
        link = self.p.with_name("lien")
        link.symlink_to(self.p)
        with self.assertRaises(OSError), lecteur(link):
            pass
        with self.assertRaises(FileExistsError):
            creer(link, self.n, self.pe)
        with lecteur(self.p) as f:
            verifier(f, self.n, self.pe)

    def test_fichier_creux_et_repli_dense(self):
        with patch("os.lseek", side_effect=OSError(errno.EINVAL, "indisponible")):
            self.assertEqual(list(etendues(3, 512)), [(0, 512)])
        with patch("os.lseek", side_effect=OSError(errno.ENXIO, "fin des données")):
            self.assertEqual(list(etendues(3, 512)), [])
        with patch("os.lseek", side_effect=[128, OSError(errno.ENXIO, "fin du trou")]):
            self.assertEqual(list(etendues(3, 512)), [(128, 512)])
        with patch("os.lseek", side_effect=[128, 128]), self.assertRaises(ValueError):
            list(etendues(3, 512))


if __name__ == "__main__":
    unittest.main()
