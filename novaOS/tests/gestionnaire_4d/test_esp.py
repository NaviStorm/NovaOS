"""La copie EFI doit restituer aussi les secteurs nuls du nouvel ELF."""
import hashlib
import io
from pathlib import Path
import struct
import sys
import tempfile
import unittest

NOVA=Path(__file__).resolve().parents[2]
sys.path[:0]=[str(NOVA/"scripts"),str(NOVA/"tests/ethernet_v1")]
from esp_installation import construire
from relecture_esp import verifier


class SecteurNul(unittest.TestCase):
    def test_remplacement_et_relecture(self):
        image=bytearray(2048)
        image[:6]=b"\x7fELF\x02\x01"
        image[18:20]=b"\xb7\0"
        image[-1]=42
        attendu=hashlib.sha256(image).hexdigest()
        with tempfile.TemporaryDirectory() as dossier:
            elf=Path(dossier)/"fixture.elf"
            elf.write_bytes(image)
            def records(nuls):
                raw=construire(elf,512,scenario="net-arm-p9",inclure_secteurs_nuls=nuls)
                return {struct.unpack_from("<I",raw,i)[0]:raw[i+4:i+516] for i in range(0,len(raw),516)}
            tous=records(True)
            compact=records(False)
        copies_manquantes=tous.keys()-compact.keys()
        self.assertGreaterEqual(len(copies_manquantes),2)
        disque=io.BytesIO()
        for i,data in compact.items():
            disque.seek(i*512)
            disque.write(data)
        for i in copies_manquantes:
            disque.seek(i*512)
            disque.write(bytes([0xa5])*512)
        with self.assertRaisesRegex(ValueError,"différent"):
            verifier(disque,0,attendu)
        for i in copies_manquantes:
            disque.seek(i*512)
            disque.write(tous[i])
        self.assertEqual(verifier(disque,0,attendu),attendu)


if __name__=="__main__":
    unittest.main()
