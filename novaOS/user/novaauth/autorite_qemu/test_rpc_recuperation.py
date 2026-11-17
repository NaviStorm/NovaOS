"""Contrôles du port applicatif TLS : portée, encodage et consultation seule."""
import sqlite3
import unittest
from recuperation import Refus,digest
from rpc_recuperation import Dispatcher,header,decode_header
from cles_recuperation import Enrollment
import test_detenteurs_v2 as fixtures
from test_detenteurs_v2 import pub

class RPC(unittest.TestCase):
    def setUp(self):
        self.fixture=fixtures.Deliveries("test_deux_remises_et_activation")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        f=self.fixture
        def impossible(*args):
            raise AssertionError("aucune mutation attendue")
        def retrieve(role,*args):
            return f.retrieve(role)[0]
        self.rpc=Dispatcher(f.f.authority,Enrollment(f.f.authority,f.f.executable),
            b"s"*16,b"d"*16,pub(f.f.server),b"m"*32,
            {digest(f.f.manifest):f.f.manifest},{digest(f.f.manifest):f.f.cipher},
            impossible,impossible,retrieve)
        self.rpc.context=f.c
        self.rpc.server_signature=f.server
        self.rpc.admin_signature=f.admin
        self.rpc.possession=f.proof
        self.prefix=b"s"*16+b"d"*16+f.c.new

    def test_encadrement_borne_et_canonique(self):
        self.assertEqual(decode_header(header(1,10,2032)),(1,10,2032))
        for bad in (header(1,10,0)+b"x",b"NARPC001"+header(1,10,0)[8:],
                    bytes(16),header(1,10,0)[:15]):
            with self.assertRaises(Refus):decode_header(bad)
        for values in ((0,10,0),(1,0,0),(1,10,2033),(2**32,10,0)):
            with self.assertRaises(Refus):header(*values)

    def test_sequence_et_instance_non_substituables(self):
        self.assertEqual(len(self.rpc.request(1,10,self.prefix+b"n"*32)),176)
        with self.assertRaises(Refus):self.rpc.request(1,10,self.prefix+b"n"*32)
        with self.assertRaises(Refus):self.rpc.request(2,10,self.prefix[:32]+b"w"*32+b"n"*32)
        with self.assertRaises(Refus):self.rpc.request(3,10,b"x"*16+self.prefix[16:]+b"n"*32)
        with self.assertRaises(Refus):self.rpc.request(4,10,self.prefix+b"n"*33)

    def test_cles_courantes_et_taille_historique(self):
        epoch=(2).to_bytes(8,"big");fresh=b"n"*32
        result=self.rpc.request(1,11,self.prefix+epoch+fresh)
        self.assertEqual(len(result),432)
        self.assertEqual(result[336:368],fresh)
        with self.assertRaises(Refus):self.rpc.request(2,11,self.prefix+epoch+fresh+b"x")

    def test_consultation_retrouve_sans_nouvelle_mutation(self):
        f=self.fixture
        first=f.deliver(1)[0]
        with sqlite3.connect(f.f.db) as db:
            before=db.execute("SELECT * FROM recovery_events").fetchall()
        fresh=b"n"*32
        proof=f.f.proof.sign(b"NovaAuth/recovery/retrieve/v2\0"+f.c.hash()+b"\x01"+fresh)
        query=f.c.op+b"\x01"+fresh+proof
        self.assertEqual(self.rpc.request(1,22,self.prefix+query),first)
        with sqlite3.connect(f.f.db) as db:
            self.assertEqual(db.execute("SELECT * FROM recovery_events").fetchall(),before)
        with self.assertRaises(Refus):
            self.rpc.request(2,22,self.prefix+query[:-1]+bytes([query[-1]^1]))
        with self.assertRaises(Refus):
            self.rpc.request(3,22,self.prefix+f.c.op+b"\x02"+fresh+proof)

if __name__=="__main__":
    unittest.main()
