"""Contre-exemples injectés dans une copie en mémoire des preuves QEMU publiques."""

import json
import struct
import tempfile
import unittest
import zipfile
from pathlib import Path

import qualification as qual


class Reception(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.archive = zipfile.ZipFile(qual.BASE / "observations.zip")
        cls.addClassCleanup(cls.archive.close)

    def lire(self, n):
        return self.archive.read("inactif-nominal/" + n)

    def verifier_remise(self, changements):
        def lire(n):
            return changements[n] if n in changements else self.lire(n)

        with tempfile.TemporaryDirectory() as tmp:
            return qual.remise(
                "inactif",
                "nominal",
                json.loads(lire("bilan.json")),
                lire,
                Path(tmp),
                None,
            )

    def test_nominal(self):
        co = self.verifier_remise({})
        self.assertEqual(len(co["ressources"]), 13)

    def test_champ_tronque(self):
        with self.assertRaisesRegex(ValueError, "Limite du champ"):
            qual.champs(self.lire("livraison.bin")[:-1])

    def test_champ_supplementaire(self):
        with self.assertRaisesRegex(ValueError, "supplémentaires"):
            qual.champs(self.lire("livraison.bin") + b"x")

    def test_depassement_avant_lecture(self):
        with self.assertRaisesRegex(ValueError, "Limite du champ"):
            qual.champs(bytes(232) + struct.pack("<I", 2**32 - 1))

    def test_bail_autre_epoque(self):
        autre = self.archive.read("actif-nominal/livraison.bin")
        with self.assertRaisesRegex(ValueError, "Liaison entre reçus"):
            self.verifier_remise(
                {"livraison.bin": autre[:232] + self.lire("livraison.bin")[232:]}
            )

    def test_chemin_substitue(self):
        with self.assertRaisesRegex(ValueError, "Origine"):
            self.verifier_remise({"origine.bin": bytes(70)})

    def test_table_modifiee(self):
        snapshot = bytearray(self.lire("avant-load.bin"))
        snapshot[-1] ^= 1
        with self.assertRaisesRegex(ValueError, "mappings"):
            self.verifier_remise({"avant-load.bin": snapshot})

    def test_substitution_de_section(self):
        b = json.loads(self.lire("bilan.json"))
        b["avant_start"]["sections"][".text"][2] = "00" * 32
        with self.assertRaisesRegex(ValueError, "image"):
            self.verifier_remise({"bilan.json": json.dumps(b).encode()})

    def test_verdict_graphique_falsifie(self):
        p = bytearray(self.lire("apres.ppm"))
        base = len(p) - len(p.split(b"\n", 3)[3])
        p[base] ^= 1
        with self.assertRaisesRegex(ValueError, "pixels"):
            qual.decoder_ecran(
                self.lire("avant.ppm"),
                bytes(p),
                json.loads(self.lire("contrat.json"))["surface"],
                json.loads(self.lire("rapport.json")),
            )

    def test_pas_de_publication_partielle(self):
        for mode in ("actif", "inactif"):
            for cas in ("surface", "elf", "manifeste", "substitution-contrat"):
                d = self.archive.read(f"{mode}-{cas}/details.bin")
                self.assertEqual(d[72:], bytes(56))

    def test_rejeu_copie_integrale(self):
        for mode in ("actif", "inactif"):
            _, fields = qual.champs(
                self.archive.read(f"{mode}-reutilise/livraison.bin")
            )
            self.assertEqual(
                fields[0], self.archive.read(f"{mode}-nominal/transfert.bin")
            )
            self.assertNotEqual(
                fields[0][16:32],
                self.archive.read(f"{mode}-reutilise/transfert.bin")[16:32],
            )


if __name__ == "__main__":
    unittest.main()
