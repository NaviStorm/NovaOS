"""Les deux remises v2 et leurs interruptions avec un moteur HPKE natif."""
from contextlib import closing
from pathlib import Path
import sqlite3
import struct
import subprocess
import unittest
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

import test_detenteurs as legacy
from cles_recuperation import Enrollment,ENROLL
from detenteur_v2 import HolderV2,pin_receiver
from detenteur import connect
from recuperation import ContextV2,Refus,ADMIN,SERVER_BINDING,POSSESSION,RESERVE,QUERY,DELIVERY_STATE,digest,ACK,ACTIVATE
from vecteur_recuperation import seal_request,open_delivery,extract,expand

def pub(key):
    return key.public_key().public_bytes_raw()

class Deliveries(unittest.TestCase):
    def setUp(self):
        self.f=legacy.Holders("test_autorite_indisponible")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.receivers=[X25519PrivateKey.generate(),X25519PrivateKey.generate()]
        self.requests=[X25519PrivateKey.generate(),X25519PrivateKey.generate()]
        self.f.proof=Ed25519PrivateKey.generate()
        api=Enrollment(self.f.authority,self.f.executable)
        packet,envelopes=api.challenge(b"s"*16,b"d"*16,2,[pub(k) for k in self.requests])
        proofs=[]
        for i,key in enumerate(self.requests):
            path=self.f.root/str(i+1)/"request"
            path.write_bytes(key.private_bytes_raw())
            path.chmod(0o600)
            proofs.append(subprocess.check_output(
                [str(self.f.executable),"open-key-proof",str(path)],input=envelopes[i]))
        api.register(packet,self.f.signers[0].sign(ENROLL+packet),self.f.signers[1].sign(ENROLL+packet),
                     self.f.admin.sign(ENROLL+packet),proofs)
        (old,epoch,generation,head),challenge,until=self.f.authority.challenge(b"s"*16,b"d"*16)
        self.c=ContextV2(b"p"*16,b"s"*16,b"d"*16,old,b"v"*32,epoch,generation,head,
            digest(self.f.manifest),2,challenge,until,pub(self.receivers[0]),pub(self.f.proof),b"m"*32,
            pub(self.receivers[1]))
        packet=self.c.packet()
        self.server=self.f.server.sign(SERVER_BINDING+packet)
        self.admin=self.f.admin.sign(ADMIN+packet)
        self.proof=self.f.proof.sign(POSSESSION+packet)
        self.f.authority.approve(self.c,self.admin,self.server,self.proof)
        self.holders=[]
        for i in range(2):
            root=self.f.root/str(i+1)
            pin_receiver(root/"remises.sqlite",pub(self.requests[i]))
            self.holders.append(HolderV2(root/"remises.sqlite",i+1,pub(self.f.keys[i]),
                pub(self.f.signers[i]),self.f.signers[i],root/"private",root/"signing",
                self.f.executable,pub(self.f.decision),pub(self.f.server),pub(self.f.admin),[b"m"*32],
                request_key_path=root/"request",request_public=pub(self.requests[i])))

    def current(self,role):
        return lambda op,fresh:self.f.authority.observe_holder(op,role,fresh,
            self.f.signers[role-1].sign(QUERY+op+bytes([role])+fresh))

    def recorded(self,op,role,fresh):
        return self.f.authority.delivery_outcome(op,role,fresh,
            self.f.signers[role-1].sign(DELIVERY_STATE+op+bytes([role])+fresh))

    def deliver(self,role,cut=None,record=None,mutate=False):
        delivery=bytes([role])*16
        nonce=bytes([role+10])*32
        data=self.c.hash()+bytes([role])+delivery+nonce
        reservation,sig=self.f.authority.reserve(self.c.op,role,delivery,nonce,self.f.proof.sign(RESERVE+data))
        request=seal_request(pub(self.requests[role-1]),self.c,role,delivery,nonce,self.f.proof)
        if mutate:
            request=request[:-1]+bytes([request[-1]^1])
        return self.holders[role-1].deliver(self.c,self.f.manifest,delivery,nonce,self.server,self.admin,self.proof,
            self.current(role),reservation,sig,request,record or self.f.authority.record_delivery,cut=cut)

    def retrieve(self,role):
        return self.holders[role-1].outcome(self.c,self.f.manifest,self.server,self.admin,self.proof,
            self.current(role),self.recorded,retrieve=True)

    def test_deux_remises_et_activation(self):
        shares=[]
        for role in (1,2):
            envelope,_=self.deliver(role)
            body=open_delivery(self.receivers[role-1],envelope)
            self.assertEqual(body[:2],bytes([1,role]))
            shares.append(body[122:])
            with self.assertRaises(Exception):
                open_delivery(self.receivers[2-role],envelope)
            proof=self.f.proof.sign(ACK+bytes([role])+bytes([role])*16+digest(envelope))
            self.f.authority.acknowledge(self.c.op,role,digest(envelope),proof)
            if role==1:
                with self.assertRaises(Refus):
                    self.f.authority.activate(self.c.op,self.c.head,
                        self.f.proof.sign(ACTIVATE+self.c.hash()+self.c.head))
        descriptor=self.f.manifest[:386]
        key=expand(extract(digest(descriptor),b"".join(shares)),b"NovaAuth/backup/data-key/v1",32)
        for i,cipher in enumerate(self.f.cipher):
            aad=digest(descriptor)+bytes([i+1])+struct.pack(">III",i,8,3)
            self.assertEqual(AESGCM(key).decrypt(i.to_bytes(12,"big"),cipher,aad),self.f.plain[i])
        self.f.authority.activate(self.c.op,self.c.head,
            self.f.proof.sign(ACTIVATE+self.c.hash()+self.c.head))

    def test_requete_falsifiee_ne_reserve_pas_le_detenteur(self):
        for role in (1,2):
            with self.assertRaises(Refus):
                self.deliver(role,mutate=True)
            with closing(connect(self.holders[role-1].path)) as db:
                self.assertEqual(db.execute("SELECT count(*) FROM envelopes").fetchone()[0],0)

    def test_perte_acquittement_recuperation_exacte(self):
        def lost(*args):
            self.f.authority.record_delivery(*args)
            raise ConnectionError("réponse perdue")
        for role in (1,2):
            with self.assertRaisesRegex(Refus,"UNCERTAIN"):
                self.deliver(role,record=lost)
            first=self.retrieve(role)
            self.assertEqual(first,self.retrieve(role))
            with closing(connect(self.holders[role-1].path)) as db:
                self.assertEqual(db.execute("SELECT count(*) FROM envelopes").fetchone()[0],1)

    def test_coupure_avant_admission_refuse_restitution(self):
        for role in (1,2):
            with self.assertRaisesRegex(Refus,"UNCERTAIN"):
                self.deliver(role,cut="sealed")
            with self.assertRaises(Refus):
                self.retrieve(role)

    def test_coupure_apres_admission_reconcilie(self):
        for role in (1,2):
            with self.assertRaisesRegex(Refus,"UNCERTAIN"):
                self.deliver(role,cut="admitted")
            envelope,_=self.retrieve(role)
            self.assertEqual(len(envelope),324)

    def test_refus_apres_retrait_ou_mauvais_recu(self):
        self.deliver(1)
        with sqlite3.connect(self.f.db) as db:
            db.execute("UPDATE scope SET epoch=epoch+1")
        with self.assertRaises(Refus):
            self.retrieve(1)

if __name__=="__main__":
    unittest.main()
