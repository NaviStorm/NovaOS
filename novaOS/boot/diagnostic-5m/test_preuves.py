"""Contre-exemples exécutés sur copies en mémoire des pièces de livraison."""

from copy import deepcopy
import json
from pathlib import Path
import os
import struct
import tarfile
from tempfile import TemporaryDirectory
import unittest
from verifier import BASE, NOVA, signature, comptages, verifier_intervalle, verifier_db
from image import pe_info, sections_chargees


class Controles(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        v = NOVA / "docs/validation/diagnostic-uefi-5m"
        with tarfile.open(v / "supports.tar.gz") as t:
            cls.pe = t.extractfile("valide.efi").read()
            cls.flash = t.extractfile("actif.fd").read()
            cls.ajouts = json.load(t.extractfile("magasin.json"))["ajouts"]
        with tarfile.open(
            NOVA / "docs/validation/diagnostic-uefi-5j/laboratoire.tar.gz"
        ) as t:
            cls.original = t.extractfile("supports5j-b/nominal.fd").read()
        with tarfile.open(v / "observations.tar.gz") as t:
            cls.cal = json.load(t.extractfile("cal-actif.json"))
            cls.pieces = {
                m.name.split("/", 1)[1]: t.extractfile(m).read()
                for m in t
                if m.name.startswith("actif-valide/")
            }
        cls.r = json.loads(cls.pieces["resultat.json"])

    def test_flash_hors_db(self):
        verifier_db(self.original, self.flash, self.ajouts)
        for off in (0, 100, 3072, len(self.flash) - 1):
            b = bytearray(self.flash)
            b[off] ^= 1
            with self.subTest(off=off), self.assertRaises(ValueError):
                verifier_db(self.original, b, self.ajouts)
        with self.assertRaises(ValueError):
            verifier_db(self.original, self.flash, list(reversed(self.ajouts)))

    def test_lien_signature(self):
        cle = (BASE / "autorite-laboratoire.pub").read_bytes()
        with TemporaryDirectory(dir=os.environ.get("NOVAOS_TMPDIR")) as td:
            original = self.pieces["transfert.bin"]
            self.assertTrue(signature(original, cle, Path(td)))
            for off in (16, 19, 32, 48, 80, 112, 175):
                b = bytearray(original)
                b[off] ^= 1
                with self.subTest(off=off):
                    self.assertFalse(signature(b, cle, Path(td)))

    def test_entree_et_relocations(self):
        for k in ("rip", "base"):
            r = deepcopy(self.r)
            r["entree"][k] += 1
            with self.subTest(k=k), self.assertRaises(ValueError):
                verifier_intervalle(r, self.pe, self.cal, self.pieces.__getitem__)
        r = deepcopy(self.r)
        r["entree"]["sections"][".data"][2] = "00" * 32
        with self.assertRaises(ValueError):
            verifier_intervalle(r, self.pe, self.cal, self.pieces.__getitem__)

    def test_compteurs_independants(self):
        rows = [json.loads(x) for x in self.pieces["compteurs-qmp.jsonl"].splitlines()]
        for dev, stat in (
            ("interne", "rd_operations"),
            ("interne", "wr_bytes"),
            ("pflash1", "wr_operations"),
        ):
            c = deepcopy(rows)
            next(a for a in c[-1] if a["device"] == dev)["stats"][stat] += 1
            with self.subTest(dev=dev, stat=stat), self.assertRaises(ValueError):
                comptages(b"\n".join(json.dumps(x).encode() for x in c), True)

    def test_attribution_eoi_et_stockage(self):
        proof = json.loads(self.pieces["interruptions.json"])
        proof["lignes"][0]["vecteur"] = 33
        pieces = dict(self.pieces)
        pieces["interruptions.json"] = json.dumps(proof).encode()
        with self.assertRaises(ValueError):
            verifier_intervalle(self.r, self.pe, self.cal, pieces.__getitem__)
        r = deepcopy(self.r)
        r["pci_apres"]["unites"].clear()
        with self.assertRaises(ValueError):
            verifier_intervalle(r, self.pe, self.cal, self.pieces.__getitem__)

    def test_rejet_geometrie_image(self):
        bad = bytearray(self.pe)
        p = struct.unpack_from("<I", bad, 60)[0]
        struct.pack_into("<I", bad, p + 24 + 56, 0)
        with self.assertRaises(ValueError):
            sections_chargees(bad, self.cal["image"])
        bad = bytearray(self.pe)
        bad[p + 24 + 144] = 1
        with self.assertRaises(ValueError):
            pe_info(bad)


if __name__ == "__main__":
    unittest.main()
