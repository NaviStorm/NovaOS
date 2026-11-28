from copy import deepcopy
import json
import tarfile
import unittest

from verifier import NOVA, domaine, pages_modifiees, smi


class Observations(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with tarfile.open(
            NOVA / "docs/validation/diagnostic-uefi-5l/observations.tar.gz"
        ) as t:

            def lire(name):
                return t.extractfile(name).read()

            d = json.loads(lire("nominal/observation.json"))
            cls.commande = d["evenements"][2]["offset"]
            cls.retour = d["evenements"][3]["offset"]
            cls.code = d["controle"]
            cls.trace = lire("nominal/intervalle.log")
            cls.d = json.loads(lire("actif-iqe/campagne.json"))["domaine_5l"]
            cls.tables = lire("actif-iqe/tables-qi.bin")

    def test_source_smi(self):
        smi(self.trace, self.commande, self.retour, self.code)
        for avant, apres in (
            (b"after RSM", b"after SMI"),
            (b"size 1 name 'apm-io'", b"size 4 name 'apm-io'"),
            (b"apm_io_write write addr=0x0", b"apm_io_write write addr=0x1"),
        ):
            with self.subTest(avant=avant), self.assertRaises(ValueError):
                smi(
                    self.trace.replace(avant, apres),
                    self.commande,
                    self.retour,
                    self.code,
                )
        with self.assertRaises(ValueError):
            smi(self.trace, self.commande, self.retour, self.code + 4)

    def test_pages_separees(self):
        a = bytes(96)
        b = bytearray(a)
        b[32] = 1
        self.assertEqual(pages_modifiees(a, b, [(0, 4096), (8192, 16384)]), [8192])
        for spans in ([(0, 8192), (4096, 8192)], [(1, 8193)], []):
            with self.assertRaises(ValueError):
                pages_modifiees(a, b, spans)
        with self.assertRaises(ValueError):
            pages_modifiees(a, b[:-1], [(0, 4096), (8192, 16384)])

    def test_recu_divergent_et_domaine(self):
        domaine(self.d, self.tables, True)
        for cle, valeur in (
            ("wait", 4),
            ("iqh", 192),
            ("iqt", 208),
            ("root", 0),
            ("fsts", 0),
        ):
            d = deepcopy(self.d)
            d["registres"][cle] = valeur
            with self.subTest(cle=cle), self.assertRaises(ValueError):
                domaine(d, self.tables, True)
        for offset in (0, 4096 + 96 * 8, 5 * 4096, 6 * 4096 + 160, 7 * 4096):
            buf = bytearray(self.tables)
            buf[offset] ^= 1
            with self.subTest(offset=offset), self.assertRaises(ValueError):
                domaine(self.d, buf, True)
