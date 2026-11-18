"""Contrôles hôtes du provisionnement administratif du banc, pas une recette VM."""
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from acteurs_recuperation import Actor,administrator_worker
from admission_exec import ENROLL,WITHDRAW,QUERY
from reprise import Refus

class AdministrativeRoute(unittest.TestCase):
    def setUp(self):
        self.tmp=TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)/'admin'
        self.actor=Actor(administrator_worker,self.root);self.addCleanup(self.actor.close)
        self.scope=(b'S'*16,b'D'*16);self.instance=b'I'*32
    def configure(self):
        self.actor.call('configure',*self.scope,b'K'*32,b'M'*32,b'F'*32,6,b'H'*32,[])
    def test_route_requires_external_configuration(self):
        with self.assertRaises(Refus):
            self.actor.call('exec-route','enroll',*self.scope,self.instance,b'')
        self.assertFalse((self.root/'exec-route.log').exists())
    def test_signature_is_bound_and_persisted(self):
        self.configure();expected=b''
        for kind,prefix,fresh in [('enroll',ENROLL,b''),('withdraw',WITHDRAW,b''),('query',QUERY,b'N'*32)]:
            packet=b''.join(self.scope)+self.instance+fresh
            sig=self.actor.call('exec-route',kind,*self.scope,self.instance,fresh)
            Ed25519PublicKey.from_public_bytes(self.actor.keys[0]).verify(sig,prefix+packet)
            expected+=prefix+packet+sig
        self.assertEqual((self.root/'exec-route.log').read_bytes(),expected)
    def test_substitution_and_malformed_requests_are_denied(self):
        self.configure()
        for kind,server,domain,instance,fresh in [
            ('enroll',b'X'*16,self.scope[1],self.instance,b''),
            ('enroll',self.scope[0],b'X'*16,self.instance,b''),
            ('enroll',*self.scope,bytes(32),b''),
            ('enroll',*self.scope,b'I'*31,b''),
            ('enroll',*self.scope,self.instance,b'N'*32),
            ('query',*self.scope,self.instance,b''),
            ('query',*self.scope,self.instance,bytes(32)),
            ('unknown',*self.scope,self.instance,b'')]:
            with self.subTest(kind=kind,server=server,domain=domain,instance=instance,fresh=fresh):
                with self.assertRaises(Refus):self.actor.call('exec-route',kind,server,domain,instance,fresh)
        self.assertFalse((self.root/'exec-route.log').exists())

if __name__=='__main__':unittest.main()
