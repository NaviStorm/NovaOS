"""La capture refuse substitutions et corruption ; une issue se relit sans écrire."""
import hashlib
import sqlite3
import unittest
from test_recuperation import Protocol,public
from recuperation import Refus,REGISTER
from capture_rpc import CaptureStore

class Capture(unittest.TestCase):
    def setUp(self):
        self.f=Protocol("test_approbation_et_activation_deux_remises");self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.store=CaptureStore(self.f.db.parent/"depot",self.f.a,public(self.f.server),(b"s"*16,b"d"*16))
        self.scope=b"s"*16+b"d"*16+b"i"*32
        m=bytearray(self.f.manifest);m[46:78]=b"z"*32
        self.chunks=[bytes([i])*24 for i in (1,2,3)]
        m[386:]=hashlib.sha256(b"".join(self.chunks)).digest()
        self.m=bytes(m)
    def stage(self):
        for i,chunk in enumerate(self.chunks):
            self.assertEqual(self.store.request(33,self.scope+b"z"*32+i.to_bytes(4,"big")+chunk),b"")
    def register(self):
        return self.store.request(32,self.scope+self.m+self.f.server.sign(REGISTER+b"i"*32+self.m))
    def test_capture_complete_et_recu_perdu(self):
        self.stage();receipt=self.register();self.assertEqual(len(receipt),176)
        with sqlite3.connect(self.f.db) as db:before=db.execute("SELECT * FROM recovery_backups").fetchall()
        nonce=b"n"*32
        found=self.store.request(34,self.scope+hashlib.sha256(self.m).digest()+nonce)
        self.assertEqual(found[:176],receipt)
        self.f.decision.public_key().verify(found[176:],b"NovaAuth/recovery/backup-query/v1\0"+nonce+receipt)
        with sqlite3.connect(self.f.db) as db:self.assertEqual(before,db.execute("SELECT * FROM recovery_backups").fetchall())
    def test_incomplet_corrompu_substitution_et_saturation(self):
        with self.assertRaises(Refus):self.register()
        for offset in (0,16,32):
            bad=bytearray(self.scope);bad[offset]^=1
            with self.assertRaises(Refus):self.store.request(33,bytes(bad)+b"z"*32+bytes(4)+b"x"*24)
        with self.assertRaises(Refus):self.store.request(33,self.scope+b"z"*32+(48).to_bytes(4,"big")+b"x"*24)
        self.stage()
        with self.assertRaises(Refus):self.store.request(33,self.scope+b"z"*32+bytes(4)+b"x"*24)
        folder=self.store.root/(b"i".hex()*32+"-"+b"z".hex()*32)
        (folder/"00.chiffre").write_bytes(b"x"*24)
        with self.assertRaises(Refus):self.register()
if __name__=="__main__":unittest.main()
