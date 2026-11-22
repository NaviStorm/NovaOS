"""Transport R2 hôte : reçus authentifiés ; aucune preuve d'effecteur invité."""
from dataclasses import replace
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
import unittest
import administration_epochs as epochs
import admission_exec as execs
import administration_mutation_rpc as rpc
import test_administration_epochs as fixtures
import reprise


class Mutations(unittest.TestCase):
    def setUp(self):
        self.f = fixtures.Epochs()
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.peer = self.f.server.public_key().public_bytes_raw()

    def send(self, operation, data, *, administration=True):
        prefix = {rpc.PUBLISH: epochs.CHANGE, rpc.ACKNOWLEDGE: epochs.EFFECTS, rpc.OUTCOME: epochs.QUERY, rpc.HISTORY: epochs.HISTORY}[operation]
        f = self.f
        body = b''.join(f.scope) + f.incarnation + data + f.server.sign(prefix + data)
        return rpc.request(f.path, f.key, self.peer, f.scope, operation, body, administration=administration)

    def change(self, op, digest, kind, epoch=2, subject=None):
        f = self.f
        return epochs.Epochs.change(f.current(), op, digest, f.principal, 1, subject or f.alice, epoch, kind)

    def disable(self):
        packet = self.change(b'R'*16, b'N'*32, epochs.DISABLE)
        signature = self.send(rpc.PUBLISH, packet)
        f = self.f
        f.key.public_key().verify(signature, reprise.HEAD_RECEIPT + reprise.HEAD_CHANGE + packet[:160])
        return packet

    def outcome(self, fresh=b'F'*32):
        f = self.f
        packet = f.current().packet() + b'R'*16 + fresh
        reply = self.send(rpc.OUTCOME, packet)
        f.key.public_key().verify(reply[-64:], epochs.QUERY + reply[:-64])
        self.assertEqual(reply[:160], packet)
        return reply

    def ack(self, effects):
        f = self.f
        packet = f.current().packet() + b'R'*16 + bytes([effects])
        reply = self.send(rpc.ACKNOWLEDGE, packet)
        self.assertEqual(reply[8:-64], packet)
        f.key.public_key().verify(reply[-64:], epochs.EFFECTS + reply[:-64])

    def test_complete_retirement_then_new_epoch_never_reopens_old_epoch(self):
        f = self.f
        self.disable()
        with self.assertRaises(reprise.Refus):
            self.send(rpc.PUBLISH, self.change(b'F'*16, b'B'*32, epochs.REACTIVATE))
        self.ack(1); self.ack(2)
        self.assertEqual(len(self.outcome()), 225)  # pas de reçu terminal
        self.ack(4)
        terminal = self.outcome()
        self.assertEqual(len(terminal), 417)
        f.key.public_key().verify(terminal[-128:-64], epochs.RETIRED + terminal[161:289])
        self.send(rpc.PUBLISH, self.change(b'F'*16, b'B'*32, epochs.REACTIVATE))
        current = f.current()
        old = replace(f.ticket, generation=current.generation, head=current.head)
        with self.assertRaisesRegex(reprise.Refus, 'époque'):
            f.admit(old)
        new = replace(old, authentication_epoch=current.generation, session=b'J'*32)
        f.admit(new)

    def test_lost_publication_ack_and_readonly_outcome(self):
        f = self.f
        self.disable()  # reçu perdu ; la mutation existe une seule fois
        self.outcome(); self.outcome(b'G'*32)
        self.ack(7)  # acquittement également perdu
        first = self.outcome(b'H'*32)
        second = self.outcome(b'I'*32)
        self.assertEqual(first[160:-64], second[160:-64])
        with reprise.transaction(f.path) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='admin-change'").fetchone(), (1,))
            self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='admin-effects'").fetchone(), (1,))
            self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='subject-retired'").fetchone(), (1,))
        with self.assertRaises(reprise.Refus): self.ack(7)
        with self.assertRaises(reprise.Refus): self.outcome(bytes(32))

    def test_ack_is_not_terminal_until_admitted_creation_is_demonstrated(self):
        f = self.f
        f.admit(); self.disable(); self.ack(7)
        pending = self.outcome()
        self.assertEqual(len(pending),225)
        rpc.progress(f.api,*f.scope)
        self.assertEqual(len(self.outcome()),225)
        with self.assertRaises(reprise.Refus):
            self.send(rpc.PUBLISH,self.change(b'F'*16,b'B'*32,epochs.REACTIVATE))
        f.finish(state=execs.CANCELLED,child=bytes(32))
        rpc.progress(f.api,*f.scope)
        self.assertEqual(len(self.outcome()),417)
        with self.assertRaises(reprise.Refus): f.admit()

    def test_two_publications_from_same_head_cannot_overwrite(self):
        f = self.f
        packets=[self.change(bytes([i])*16,bytes([i])*32,epochs.UPDATE,epoch=0,subject=bytes(16)) for i in (10,11)]
        gate=Barrier(2)
        def publish(packet):
            gate.wait(timeout=5)
            try: self.send(rpc.PUBLISH,packet); return True
            except reprise.Refus: return False
        with ThreadPoolExecutor(2) as pool:
            self.assertCountEqual(list(pool.map(publish,packets)),[True,False])
        self.assertEqual(f.current().generation,7)

    def test_no_implicit_route_no_substitution_no_unsigned_effects(self):
        f=self.f
        packet=self.change(b'R'*16,b'N'*32,epochs.DISABLE)
        with self.assertRaises(reprise.Refus): self.send(rpc.PUBLISH,packet,administration=False)
        body=b''.join(f.scope)+f.incarnation+packet+f.server.sign(epochs.CHANGE+packet)
        for offset in (0,16,32,64,80,96,128,136,144,176,192,224,240,248,264,272,273):
            bad=bytearray(body);bad[offset]^=1
            with self.assertRaises(reprise.Refus):
                rpc.request(f.path,f.key,self.peer,f.scope,rpc.PUBLISH,bytes(bad),administration=True)
        for bad in (body[:-1],body+b'0',body[:64]):
            with self.assertRaises(reprise.Refus):
                rpc.request(f.path,f.key,self.peer,f.scope,rpc.PUBLISH,bad,administration=True)
        self.assertEqual(f.current().generation,6)
        self.disable()
        with self.assertRaises(reprise.Refus): self.ack(8)
        self.assertEqual(len(self.outcome()),225)

    def test_history_is_readonly_and_binds_original_principal_to_fresh_query(self):
        f=self.f
        original=self.disable()
        query=f.current().packet()+b'R'*16+b'F'*32
        first=self.send(rpc.HISTORY,query)
        f.key.public_key().verify(first[-64:],epochs.HISTORY+first[:-64])
        self.assertEqual(len(first),434)
        self.assertEqual(first[:160],query)
        self.assertEqual(first[160:369],original)
        self.assertEqual(first[320:336],f.principal)
        self.ack(7)
        second=self.send(rpc.HISTORY,query[:-32]+b'G'*32)
        self.assertEqual(len(second),626)
        self.assertEqual(second[160:369],original)
        f.key.public_key().verify(second[-64:],epochs.HISTORY+second[:-64])
        with reprise.transaction(f.path) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='admin-change'").fetchone(),(1,))
            self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='subject-retired'").fetchone(),(1,))
        with self.assertRaises(reprise.Refus): self.send(rpc.HISTORY,query,administration=False)
        with self.assertRaises(reprise.Refus): self.send(rpc.HISTORY,query[:128]+bytes(32))

if __name__ == '__main__': unittest.main()
