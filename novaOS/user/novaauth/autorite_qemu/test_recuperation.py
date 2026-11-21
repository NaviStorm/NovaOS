"""Contrôles hôtes de l'ordre durable, distincts des recettes VFS invitées."""
import dataclasses
from hashlib import sha256
from pathlib import Path
import sqlite3
import struct
import tempfile
import unittest

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from reprise import Recovery, Refus, initialize as initialize_authority
from recuperation import (
    RecoveryAuthority, Context, initialize, SERVER_BINDING, POSSESSION, ADMIN,
    REGISTER, RESERVE, ACK, ACTIVATE, QUERY, ROTATE,
)

def key():
    return Ed25519PrivateKey.generate()

def public(k):
    return k.public_key().public_bytes_raw()

class Protocol(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.db=Path(self.tmp.name)/"autorite.sqlite"
        self.decision,self.server,self.admin,self.possession=key(),key(),key(),key()
        self.gsign,self.asign=key(),key()
        self.gpublic,self.apublic=public(key()),public(key())
        initialize_authority(self.db,public(self.decision))
        self.old=Recovery(self.db,self.decision)
        self.old.provision(b"s"*16,b"d"*16,b"i"*32,13,b"h"*32,public(self.server),public(self.admin))
        initialize(self.db)
        self.time=1_000_000_000
        self.a=RecoveryAuthority(self.old,[b"m"*32],lambda:self.time)
        self.a.provision_keys(b"s"*16,b"d"*16,2,
                              [self.gpublic,self.apublic,public(self.gsign),public(self.asign)])
        manifest=bytearray(418)
        manifest[:14]=b"NABACK01\x00\x20\x00\x01\xff\xff"
        manifest[14:30]=b"s"*16;manifest[30:46]=b"d"*16
        manifest[46:78]=b"b"*32
        manifest[78:94]=struct.pack(">QQ",13,2)
        manifest[94:126]=sha256(self.gpublic).digest()
        manifest[126:158]=sha256(self.apublic).digest()
        manifest[158:174]=b"d"*16
        manifest[246:254]=struct.pack(">Q",13)
        manifest[270:302]=b"h"*32
        manifest[302:366]=b"e"*64
        manifest[366:386]=struct.pack(">IIIII",8,8,8,3,1024)
        manifest[386:]=b"c"*32
        self.manifest=bytes(manifest)
        self.a.register_backup(self.manifest,b"i"*32,self.server.sign(REGISTER+b"i"*32+self.manifest))
        self.context=self.fresh()

    def fresh(self, instance=b"n"*32, operation=b"o"*16):
        (old,epoch,generation,head),nonce,until=self.a.challenge(b"s"*16,b"d"*16)
        return Context(operation,b"s"*16,b"d"*16,old,instance,epoch,generation,head,
                       sha256(self.manifest).digest(),2,nonce,until,b"x"*32,public(self.possession),b"m"*32)

    def approve(self,c=None):
        c=c or self.context
        p=c.packet()
        return self.a.approve(c,self.admin.sign(ADMIN+p),self.server.sign(SERVER_BINDING+p),
                              self.possession.sign(POSSESSION+p))

    def reserve(self,role):
        c=self.context
        delivery=bytes([role])*16;nonce=bytes([role+10])*32
        data=c.hash()+bytes([role])+delivery+nonce
        return self.a.reserve(c.op,role,delivery,nonce,self.possession.sign(RESERVE+data))

    def delivered(self,role):
        self.reserve(role)
        c=self.context
        # Chiffré opaque synthétique. Les tests natifs vérifient réellement HPKE.
        prefix=b"NASHAR01"+bytes([1,role])+c.hash()+bytes([role])*16+b"e"*32+b"z"*170
        signer=[self.gsign,self.asign][role-1]
        envelope=prefix+signer.sign(prefix)
        digest=sha256(envelope).digest()
        self.a.record_delivery(c.op,role,envelope,signer.sign(RESERVE+c.hash()+digest))
        proof=self.possession.sign(ACK+bytes([role])+bytes([role])*16+digest)
        self.a.acknowledge(c.op,role,digest,proof)
        self.a.acknowledge(c.op,role,digest,proof)
        return envelope

    def test_approbation_et_activation_deux_remises(self):
        self.approve()
        c=self.context
        proof=self.possession.sign(ACTIVATE+c.hash()+c.head)
        with self.assertRaises(Refus): self.a.activate(c.op,c.head,proof)
        self.delivered(1)
        with self.assertRaises(Refus): self.a.activate(c.op,c.head,proof)
        self.delivered(2)
        data,sig=self.a.activate(c.op,c.head,proof)
        self.decision.public_key().verify(sig,ACTIVATE+data)
        with self.assertRaises(Refus): self.a.activate(c.op,c.head,proof)

    def test_perte_reponse_reconciliation_sans_republication(self):
        first=self.approve()
        c=self.context
        with self.assertRaises(Refus): self.approve()
        fresh=b"q"*32
        data,sig=self.a.outcome(c.op,fresh,self.possession.sign(QUERY+c.op+fresh))
        self.decision.public_key().verify(sig,QUERY+data)
        self.assertIn(first[0]+first[1],data)
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM recovery_operations").fetchone()[0],1)

    def test_une_seule_approbation_concurrente(self):
        second=self.fresh(b"j"*32,b"p"*16)
        self.approve()
        with self.assertRaises(Refus): self.approve(second)
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute("SELECT instance FROM scope").fetchone()[0],b"n"*32)

    def test_defi_expire(self):
        self.time=self.context.until
        with self.assertRaises(Refus):self.approve()

    def test_instances_cles_et_domaines_substitues(self):
        for field,value in [("new",b"a"*32),("receiver",b"a"*32),("server",b"a"*16),
                            ("domain",b"a"*16),("measurement",b"a"*32)]:
            c=dataclasses.replace(self.context,**{field:value})
            p=c.packet()
            # L'attaquant A peut approuver, mais pas fabriquer l'attestation serveur.
            original=self.context.packet()
            with self.assertRaises(Refus):
                self.a.approve(c,self.admin.sign(ADMIN+p),
                               self.server.sign(SERVER_BINDING+original),
                               self.possession.sign(POSSESSION+p))

    def test_ancienne_generation_reste_refusee(self):
        with sqlite3.connect(self.db) as db:
            db.execute("UPDATE scope SET generation=14,head=?",(b"k"*32,))
        with self.assertRaises(Refus):self.approve()
        with self.assertRaises(Refus):
            self.a.register_backup(self.manifest,b"i"*32,self.server.sign(REGISTER+b"i"*32+self.manifest))

    def test_reprise_et_mutation_en_concurrence(self):
        self.approve()
        with sqlite3.connect(self.db) as db:
            db.execute("UPDATE scope SET generation=14,head=?",(b"k"*32,))
        with self.assertRaises(Refus):self.reserve(1)

    def test_reservation_unique_et_rejeu(self):
        self.approve()
        self.reserve(1)
        with self.assertRaises(sqlite3.IntegrityError):self.reserve(1)
        with self.assertRaises(Refus):self.reserve(3)

    def test_revocation_epoque_et_retrait_route(self):
        self.approve()
        c=self.context
        data=c.server+c.domain+struct.pack(">Q",2)+b"revoke"
        with self.assertRaises(Refus):
            self.a.revoke_key_epoch(c.server,c.domain,2,self.gsign.sign(ROTATE+data),self.gsign.sign(ROTATE+data))
        self.a.revoke_key_epoch(c.server,c.domain,2,self.gsign.sign(ROTATE+data),self.asign.sign(ROTATE+data))
        with self.assertRaises(Refus):self.reserve(1)

    def test_redemarrage_autorite_ne_ressuscite_pas_operation(self):
        self.approve()
        c=self.context
        self.a=RecoveryAuthority(self.old,[b"m"*32],lambda:self.time)
        with self.assertRaises(Refus):self.reserve(1)
        fresh=b"q"*32
        data,sig=self.a.outcome(c.op,fresh,self.possession.sign(QUERY+c.op+fresh))
        self.decision.public_key().verify(sig,QUERY+data)

    def test_recu_historique_ne_vaut_pas_autorisation_courante(self):
        self.approve()
        c=self.context;fresh=b"q"*32;proof=self.possession.sign(QUERY+c.op+fresh)
        first=self.a.observe(c.op,fresh,proof)
        self.time+=300_000_000_001
        with self.assertRaises(Refus):self.a.observe(c.op,b"r"*32,self.possession.sign(QUERY+c.op+b"r"*32))
        self.decision.public_key().verify(first[1],QUERY+first[0])

if __name__=="__main__":
    unittest.main()
