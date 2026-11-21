"""Interopérabilité OpenSSL / Rust et interruptions des détenteurs QEMU."""
from contextlib import closing
from hashlib import sha256
from pathlib import Path
import os
import sqlite3
import struct
import tempfile
import unittest

from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
import detenteur
from reprise import initialize as initialize_authority,Recovery,Refus
from recuperation import RecoveryAuthority,Context,initialize,REGISTER,ADMIN,POSSESSION,SERVER_BINDING,RESERVE,QUERY
from vecteur_recuperation import capture,open_delivery,extract,expand

def pub(key):
    return key.public_key().public_bytes_raw()

class Holders(unittest.TestCase):
    def setUp(self):
        self.executable=Path(os.environ["NOVA_DETENTEUR"]).resolve(strict=True)
        self.tmp=tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)
        self.keys=[X25519PrivateKey.generate(),X25519PrivateKey.generate()]
        self.signers=[Ed25519PrivateKey.generate(),Ed25519PrivateKey.generate()]
        self.decision,self.server,self.admin,self.proof=[Ed25519PrivateKey.generate() for _ in range(4)]
        self.recipient=X25519PrivateKey.generate()
        self.head=b"d"*16+bytes(72)+struct.pack(">Q",13)+b"q"*16+b"h"*32
        self.manifest,self.cipher,self.plain=capture(self.keys,self.head)
        self.db=self.root/"autorite.sqlite"
        initialize_authority(self.db,pub(self.decision))
        old=Recovery(self.db,self.decision)
        old.provision(b"s"*16,b"d"*16,b"i"*32,13,b"h"*32,pub(self.server),pub(self.admin))
        initialize(self.db)
        self.authority=RecoveryAuthority(old,[b"m"*32])
        self.authority.provision_keys(b"s"*16,b"d"*16,2,[*map(pub,self.keys),*map(pub,self.signers)])
        self.authority.register_backup(self.manifest,b"i"*32,self.server.sign(REGISTER+b"i"*32+self.manifest))
        (old,epoch,generation,head),nonce,until=self.authority.challenge(b"s"*16,b"d"*16)
        self.c=Context(b"o"*16,b"s"*16,b"d"*16,old,b"n"*32,epoch,generation,head,
                       sha256(self.manifest).digest(),2,nonce,until,pub(self.recipient),pub(self.proof),b"m"*32)
        packet=self.c.packet()
        self.admin_sig=self.admin.sign(ADMIN+packet)
        self.server_sig=self.server.sign(SERVER_BINDING+packet)
        self.pop=self.proof.sign(POSSESSION+packet)
        self.authority.approve(self.c,self.admin_sig,self.server_sig,self.pop)
        self.holders=[]
        for i in range(2):
            root=self.root/str(i+1);root.mkdir(mode=0o700)
            (root/"private").write_bytes(self.keys[i].private_bytes_raw())
            (root/"signing").write_bytes(self.signers[i].private_bytes_raw())
            detenteur.initialize(root/"remises.sqlite",i+1,pub(self.keys[i]),pub(self.signers[i]))
            self.holders.append(detenteur.Holder(root/"remises.sqlite",i+1,pub(self.keys[i]),pub(self.signers[i]),self.signers[i],
                root/"private",root/"signing",self.executable,pub(self.decision),pub(self.server),pub(self.admin),[b"m"*32]))

    def current(self,op,fresh):
        return self.authority.observe(op,fresh,self.proof.sign(QUERY+op+fresh))

    def deliver(self,role,cut=None):
        delivery=bytes([role])*16;nonce=bytes([10+role])*32
        data=self.c.hash()+bytes([role])+delivery+nonce
        reserved,sig=self.authority.reserve(self.c.op,role,delivery,nonce,self.proof.sign(RESERVE+data))
        return self.holders[role-1].deliver(self.c,self.manifest,delivery,nonce,self.server_sig,self.admin_sig,
                                          self.pop,self.current,reserved,sig,cut=cut)

    def retrieve(self,role):
        return self.holders[role-1].outcome(self.c,self.manifest,self.server_sig,self.admin_sig,self.pop,
                                          self.current,retrieve=True)

    def test_interop_deux_detenteurs_et_dechiffrement(self):
        shares=[]
        for role in [1,2]:
            envelope,_=self.deliver(role)
            body=open_delivery(self.recipient,envelope)
            self.assertEqual(body[:2],bytes([1,role]))
            self.assertEqual(body[2:34],self.c.hash())
            self.assertEqual(body[34:66],sha256(self.manifest).digest())
            shares.append(body[122:])
        descriptor=self.manifest[:386]
        key=expand(extract(sha256(descriptor).digest(),b"".join(shares)),b"NovaAuth/backup/data-key/v1",32)
        for i,cipher in enumerate(self.cipher):
            aad=sha256(descriptor).digest()+bytes([i+1])+struct.pack(">III",i,8,3)
            self.assertEqual(AESGCM(key).decrypt(i.to_bytes(12,"big"),cipher,aad),self.plain[i])

    def test_une_part_ne_suffit_pas(self):
        for role in [1,2]:
            envelope,_=self.deliver(role)
            share=open_delivery(self.recipient,envelope)[122:]
            halves=[bytes(32),bytes(32)];halves[role-1]=share
            descriptor=self.manifest[:386]
            key=expand(extract(sha256(descriptor).digest(),b"".join(halves)),b"NovaAuth/backup/data-key/v1",32)
            aad=sha256(descriptor).digest()+b"\1"+struct.pack(">III",0,8,3)
            with self.assertRaises(Exception):AESGCM(key).decrypt(bytes(12),self.cipher[0],aad)

    def test_coupure_apres_scellement_retourne_meme_enveloppe(self):
        for role in [1,2]:
            with self.assertRaisesRegex(Refus,"UNCERTAIN"):self.deliver(role,cut="sealed")
            first=self.retrieve(role)
            second=self.retrieve(role)
            self.assertEqual(first,second)
            with closing(detenteur.connect(self.holders[role-1].path)) as db:
                self.assertEqual(db.execute("SELECT count(*) FROM envelopes").fetchone()[0],1)

    def test_coupure_apres_reservation_aucune_remise_implicite(self):
        for role in [1,2]:
            with self.assertRaisesRegex(Refus,"UNCERTAIN"):self.deliver(role,cut="reserved")
            with self.assertRaises(Refus):self.retrieve(role)
            with closing(detenteur.connect(self.holders[role-1].path)) as db:
                self.assertIsNone(db.execute("SELECT ciphertext FROM envelopes").fetchone()[0])

    def test_refus_restitution_apres_retrait(self):
        self.deliver(1)
        with sqlite3.connect(self.db) as db:db.execute("UPDATE scope SET epoch=epoch+1")
        with self.assertRaises(Refus):self.retrieve(1)

    def test_autorite_indisponible(self):
        self.deliver(1)
        def offline(op,fresh):raise ConnectionError("autorité injoignable")
        with self.assertRaises(ConnectionError):
            self.holders[0].outcome(self.c,self.manifest,self.server_sig,self.admin_sig,self.pop,offline,retrieve=True)

    def test_mauvaise_cle_detenteur(self):
        self.holders[0].key_path.write_bytes(self.keys[1].private_bytes_raw())
        with self.assertRaises(Refus):self.deliver(1)
        with closing(detenteur.connect(self.holders[0].path)) as db:
            self.assertIsNone(db.execute("SELECT ciphertext FROM envelopes").fetchone()[0])

if __name__=="__main__":unittest.main()
