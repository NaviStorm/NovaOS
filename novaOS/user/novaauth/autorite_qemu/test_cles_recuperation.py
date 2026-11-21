"""Enregistrement à double contrôle et contexte v2, avec défis HPKE natifs."""
import dataclasses
from pathlib import Path
import os
import sqlite3
import subprocess
import threading
import unittest
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey

import test_recuperation as legacy
from recuperation import Context, ContextV2, ADMIN, SERVER_BINDING, POSSESSION, Refus
from cles_recuperation import Enrollment, ENROLL, OBSERVE, RECORD

class Keys(unittest.TestCase):
    def setUp(self):
        self.fixture = legacy.Protocol("test_defi_expire")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.f = self.fixture
        self.tool = Path(os.environ["NOVA_DETENTEUR"])
        self.api = Enrollment(self.f.a,self.tool,lambda:self.f.time)
        self.keys = [X25519PrivateKey.generate(),X25519PrivateKey.generate()]
        self.paths = []
        for index,key in enumerate(self.keys):
            path = self.f.db.parent/f"receiver-{index}.key"
            path.write_bytes(key.private_bytes_raw())
            path.chmod(0o600)
            self.paths.append(path)
        self.packet,self.envelopes = self.api.challenge(b"s"*16,b"d"*16,2,
            [key.public_key().public_bytes_raw() for key in self.keys])
        self.proofs = [subprocess.check_output(
            [str(self.tool),"open-key-proof",str(path)],input=envelope)
            for path,envelope in zip(self.paths,self.envelopes)]

    def register(self,packet=None,proofs=None):
        packet = packet or self.packet
        return self.api.register(packet,
            self.f.gsign.sign(ENROLL+packet),self.f.asign.sign(ENROLL+packet),
            self.f.admin.sign(ENROLL+packet),proofs or self.proofs)

    def context(self):
        fields = self.f.context.__dict__.copy()
        fields["receiver"] = self.keys[0].public_key().public_bytes_raw()
        return ContextV2(**fields,receiver_administrator=self.keys[1].public_key().public_bytes_raw())

    def approve(self,c):
        p = c.packet()
        return self.f.a.approve(c,self.f.admin.sign(ADMIN+p),self.f.server.sign(SERVER_BINDING+p),
                                self.f.possession.sign(POSSESSION+p))

    def test_format_enregistrement_et_contexte(self):
        self.assertEqual(RECORD.size,272)
        packet,signature = self.register()
        self.f.decision.public_key().verify(signature,ENROLL+packet)
        c = self.context()
        self.assertEqual(len(c.packet()),376)
        self.assertEqual(Context.decode(c.packet()),c)
        self.assertNotEqual(c.recipient(1),c.recipient(2))
        self.approve(c)

    def test_v2_refuse_epoque_incomplete(self):
        with self.assertRaises(Refus):
            self.approve(self.context())
        self.register()
        with sqlite3.connect(self.f.db) as db:
            db.execute("DELETE FROM recovery_receivers WHERE role=2")
        with self.assertRaises(Refus):
            self.approve(self.context())

    def test_substitution_cle_et_defi(self):
        packet = bytearray(self.packet)
        packet[112] ^= 1
        with self.assertRaises(Refus):
            self.register(bytes(packet))
        proofs = self.proofs[::-1]
        with self.assertRaises(Refus):
            self.register(proofs=proofs)
        self.register()

    def test_double_controle_obligatoire(self):
        p = self.packet
        with self.assertRaises(Refus):
            self.api.register(p,self.f.gsign.sign(ENROLL+p),self.f.gsign.sign(ENROLL+p),
                              self.f.admin.sign(ENROLL+p),self.proofs)
        with self.assertRaises(Refus):
            self.api.register(p,self.f.gsign.sign(ENROLL+p),self.f.asign.sign(ENROLL+p),
                              self.f.possession.sign(ENROLL+p),self.proofs)
        self.register()

    def test_expiration_et_mauvaise_cle_de_reception(self):
        wrong = subprocess.run([str(self.tool),"open-key-proof",str(self.paths[1])],
                               input=self.envelopes[0],capture_output=True)
        self.assertNotEqual(wrong.returncode,0)
        self.assertEqual(wrong.stdout,b"")
        self.f.time += 30_000_000_000
        with self.assertRaises(Refus):
            self.register()

    def test_reponse_perdue_retrouvee_sans_remplacement(self):
        receipt = self.register()
        with self.assertRaises(Refus):
            self.register()
        fresh = b"z"*32
        observed,signature = self.api.observe(b"s"*16,b"d"*16,2,fresh)
        self.f.decision.public_key().verify(signature,OBSERVE+observed)
        self.assertEqual(observed,receipt[0]+receipt[1]+fresh)
        with sqlite3.connect(self.f.db) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM recovery_receivers").fetchone()[0],2)

    def test_deux_enregistrements_concurrents(self):
        barrier = threading.Barrier(2)
        results = []
        def run():
            barrier.wait()
            try:
                self.register()
                results.append("accepted")
            except Refus:
                results.append("refused")
        threads = [threading.Thread(target=run) for _ in range(2)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.assertCountEqual(results,["accepted","refused"])

    def test_cles_instance_liees_a_approbation(self):
        self.register()
        original = self.context()
        changed = dataclasses.replace(original,receiver_administrator=b"v"*32)
        with self.assertRaises(Refus):
            self.f.a.approve(changed,self.f.admin.sign(ADMIN+changed.packet()),
                self.f.server.sign(SERVER_BINDING+original.packet()),
                self.f.possession.sign(POSSESSION+changed.packet()))
        self.approve(original)

if __name__ == "__main__":
    unittest.main()
