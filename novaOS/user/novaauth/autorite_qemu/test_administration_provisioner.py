"""Enrôlement initial explicite hors invité ; aucune qualification QEMU implicite."""
from dataclasses import replace
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey,Ed25519PublicKey
from acteurs_recuperation import Actor, administrator_worker
from recuperation import ContextV2, SERVER_BINDING, POSSESSION
from reprise import Published,Refus
from administration_epochs import Epochs, ENROLL

class Provisioner(unittest.TestCase):
    def setUp(self):
        self.tmp=TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.actor=Actor(administrator_worker,Path(self.tmp.name)/'admin');self.addCleanup(self.actor.close)
        self.server=Ed25519PrivateKey.generate();self.possession=Ed25519PrivateKey.generate()
        pub=lambda k:k.public_key().public_bytes_raw()
        self.c=ContextV2(op=b'O'*16,server=b'S'*16,domain=b'D'*16,old=b'I'*32,new=b'J'*32,route_epoch=2,
            generation=7,head=b'H'*32,manifest=b'M'*32,key_epoch=1,challenge=b'C'*32,until=10000,
            receiver=b'R'*32,possession=pub(self.possession),measurement=b'E'*32,receiver_administrator=b'T'*32)
        c=self.c
        self.actor.call('configure',c.server,c.domain,pub(self.server),c.measurement,c.manifest,c.generation,c.head,[bytes([i])*32 for i in range(1,7)])
        self.subjects=((b'A'*16,1),(b'B'*16,2))
        self.state=Published(c.server,c.domain,c.new,c.route_epoch+1,c.generation,c.head)
    def approve(self):
        c=self.c;packet=c.packet()
        self.actor.call('approve',c,self.server.sign(SERVER_BINDING+packet),self.possession.sign(POSSESSION+packet))
    def test_approbation_epoque_exacte_et_reponse_perdue(self):
        with self.assertRaises(Refus):self.actor.call('r2-enroll',self.state)
        self.actor.call('r2-subjects',self.subjects);self.approve()
        proof=self.actor.call('r2-enroll',self.state)
        Ed25519PublicKey.from_public_bytes(self.actor.keys[0]).verify(proof,ENROLL+Epochs.enrollment(self.state,self.subjects))
        self.assertEqual(self.actor.call('r2-enroll',self.state),proof)
        for wrong in (replace(self.state,instance=self.c.old),replace(self.state,epoch=2),replace(self.state,generation=6),replace(self.state,head=b'X'*32),replace(self.state,domain=b'X'*16)):
            with self.assertRaises(Refus):self.actor.call('r2-enroll',wrong)
        with self.assertRaises(Refus):self.actor.call('r2-subjects',((b'B'*16,7),))
    def test_table_non_substituable_et_bornee(self):
        for wrong in ((),((b'A'*16,0),),((b'A'*16,8),),((b'A'*16,1),(b'A'*16,2))):
            with self.assertRaises(Refus):self.actor.call('r2-subjects',wrong)
        self.actor.call('r2-subjects',self.subjects)
        with self.assertRaises(Refus):self.actor.call('r2-subjects',((b'B'*16,7),))
        self.approve()
        self.actor.call('r2-enroll',self.state)
if __name__=='__main__':unittest.main()
