"""Tests hôtes de l'ordre durable. Ne constituent pas la recette VFS/deux VM."""
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
from dataclasses import replace
from pathlib import Path
from tempfile import TemporaryDirectory
from threading import Barrier
import unittest

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
import admission_exec as e
import reprise as r


class Admissions(unittest.TestCase):
    def setUp(self):
        self.tmp=TemporaryDirectory(); self.addCleanup(self.tmp.cleanup)
        self.path=Path(self.tmp.name)/'authority.db'
        self.server,self.admin,self.authority=[Ed25519PrivateKey.generate() for _ in range(3)]
        r.initialize(self.path,self.authority.public_key().public_bytes_raw());e.initialize(self.path)
        self.a=r.Recovery(self.path,self.authority)
        self.scope=(b'S'*16,b'D'*16);self.old=b'A'*32;self.new=b'B'*32
        self.a.provision(*self.scope,self.old,6,b'H'*32,
                         self.server.public_key().public_bytes_raw(),self.admin.public_key().public_bytes_raw())
        self.api=e.Admissions(self.a)
        data=b''.join(self.scope)+self.old
        self.api.enroll(*self.scope,self.old,self.admin.sign(e.ENROLL+data))
        self.ticket=e.Admission(*self.scope,self.old,6,b'H'*32,b'I'*32,b'L'*32,b'P'*16,b'E'*32,b'T'*16,b'O'*16)

    def admit(self,q=None):
        q=q or self.ticket
        return self.api.admit(q,self.server.sign(e.ADMIT+q.packet()))

    def finish(self,state=e.CREATED,child=b'C'*32):
        q=self.ticket
        return self.api.finish(q,state,child,self.server.sign(e.FINISH+q.finish(state,child)))

    def retire(self):
        data=b''.join(self.scope)+self.old
        return self.api.begin_withdrawal(*self.scope,self.old,self.admin.sign(e.WITHDRAW+data))

    def activate(self):
        c,d,k=self.a.issue(*self.scope,self.new,b'R'*16)
        return self.a.admit(c,d,k,self.admin.sign(r.ADMIN+c.packet()),self.server.sign(c.possession()))

    def query(self):
        q=self.ticket;fresh=b'F'*32
        data,sig=self.api.query(q,fresh,self.server.sign(e.QUERY+q.packet()+fresh))
        self.authority.public_key().verify(sig,e.QUERY+data)
        return data

    def routes(self):
        with closing(r.open_database(self.path)) as db:
            return db.execute('SELECT incarnation,state FROM exec_routes ORDER BY incarnation').fetchall()

    def test_delayed_creation_blocks_definitive_withdrawal(self):
        p,s=self.admit();self.authority.public_key().verify(s,e.RECEIPT+p)
        begin,_=self.retire();self.assertIn(e.DRAINING.encode(),begin)
        with self.assertRaisesRegex(r.Refus,'drainage incomplet'):self.api.drain(*self.scope,self.old)
        with self.assertRaisesRegex(r.Refus,'drainage EXEC'):self.activate()
        self.assertEqual(self.routes(),[(self.old,e.DRAINING)])
        self.finish()
        p,s=self.api.drain(*self.scope,self.old)
        self.authority.public_key().verify(s,e.RECEIPT+p)
        self.assertIn(e.RETIRED.encode(),p)
        self.activate()
        self.assertEqual(self.routes(),[(self.old,e.RETIRED),(self.new,e.ACTIVE)])
        with self.assertRaises(r.Refus):self.admit(replace(self.ticket,op=b'Z'*16))
        new=replace(self.ticket,incarnation=self.new,owner=b'J'*32,session=b'M'*32,op=b'N'*16)
        self.admit(new)

    def test_no_expiry_or_restart_cancels_outstanding_admission(self):
        self.admit();self.retire()
        self.a=r.Recovery(self.path,self.authority,lambda:2**62);self.api=e.Admissions(self.a)
        self.query()
        for _ in range(3):
            with self.assertRaisesRegex(r.Refus,'drainage incomplet'):self.api.drain(*self.scope,self.old)
        self.assertEqual(self.routes(),[(self.old,e.DRAINING)])
        self.finish(e.CANCELLED,bytes(32));self.api.drain(*self.scope,self.old);self.activate()

    def test_lost_reply_queries_without_repeating_mutation(self):
        self.admit()  # deliberately discard successful durable receipt
        self.api=e.Admissions(r.Recovery(self.path,self.authority))
        before=self.query()
        with self.assertRaisesRegex(r.Refus,'déjà décidée'):self.admit()
        self.assertEqual(before,self.query())
        self.finish()
        terminal=self.query();self.assertNotEqual(before,terminal)
        with self.assertRaisesRegex(r.Refus,'déjà décidée'):self.finish(e.CANCELLED,bytes(32))
        self.assertEqual(terminal,self.query())

    def test_concurrent_consumptions_only_one_admission(self):
        barrier=Barrier(2)
        def run(_):
            barrier.wait(timeout=5)
            try:self.admit();return 'OK'
            except r.Refus:return 'REFUSED'
        with ThreadPoolExecutor(2) as pool:result=list(pool.map(run,range(2)))
        self.assertCountEqual(result,['OK','REFUSED'])
        with closing(r.open_database(self.path)) as db:
            self.assertEqual(db.execute('SELECT count(*) FROM exec_admissions').fetchone(),(1,))

    def test_admission_vs_withdrawal_has_one_durable_order(self):
        barrier=Barrier(2)
        def launch():
            barrier.wait(timeout=5)
            try:self.admit();return True
            except r.Refus:return False
        def withdraw():barrier.wait(timeout=5);self.retire()
        with ThreadPoolExecutor(2) as pool:
            f=pool.submit(launch);g=pool.submit(withdraw);admitted=f.result();g.result()
        with closing(r.open_database(self.path)) as db:
            events=db.execute('SELECT kind FROM exec_events ORDER BY sequence').fetchall()
        if admitted:
            self.assertLess(events.index(('admission',)),events.index((e.DRAINING,)))
            with self.assertRaises(r.Refus):self.api.drain(*self.scope,self.old)
        else:
            self.assertNotIn(('admission',),events);self.api.drain(*self.scope,self.old)

    def test_bound_fields_and_distinct_proofs(self):
        q=self.ticket;proof=self.server.sign(e.ADMIT+q.packet())
        for field in q.__dataclass_fields__:
            changed=replace(q,**{field:7 if field=='generation' else b'Z'*len(getattr(q,field))})
            with self.subTest(field=field),self.assertRaises(r.Refus):self.api.admit(changed,proof)
        with self.assertRaises(r.Refus):self.api.admit(q,self.admin.sign(e.ADMIT+q.packet()))
        self.admit()
        with self.assertRaises(r.Refus):self.api.finish(q,e.CANCELLED,bytes(32),proof)
        with self.assertRaises(r.Refus):q.finish(e.CANCELLED,b'C'*32)
        with self.assertRaises(r.Refus):q.finish(e.CREATED,bytes(32))
        self.assertEqual(e.Admission.decode(q.packet()),q)
        for packet in (q.packet()[:-1],q.packet()+b'0',b'BADMAGIC'+q.packet()[8:]):
            with self.assertRaises(r.Refus):e.Admission.decode(packet)

    def test_saturation_is_atomic_and_releases_pending_budget(self):
        for i in range(e.MAX_PENDING):
            q=replace(self.ticket,op=(i+1).to_bytes(16,'big'),ticket=(i+1).to_bytes(16,'big'))
            self.admit(q)
        q=replace(self.ticket,op=b'X'*16,ticket=b'X'*16)
        with self.assertRaisesRegex(r.Refus,'LIMIT'):self.admit(q)
        with closing(r.open_database(self.path)) as db:
            self.assertEqual(db.execute('SELECT count(*) FROM exec_admissions').fetchone(),(e.MAX_PENDING,))
        first=replace(self.ticket,op=(1).to_bytes(16,'big'),ticket=(1).to_bytes(16,'big'))
        self.api.finish(first,e.CANCELLED,bytes(32),self.server.sign(e.FINISH+first.finish(e.CANCELLED,bytes(32))))
        self.admit(q)

    def test_cancel_tombstone_prevents_late_admission(self):
        self.finish(e.CANCELLED,bytes(32))
        with self.assertRaisesRegex(r.Refus,'déjà décidée'):self.admit()
        self.query();self.retire();self.api.drain(*self.scope,self.old);self.activate()

    def test_lost_withdrawal_reply_only_reads_pending_operations(self):
        self.admit();self.retire()
        self.api=e.Admissions(r.Recovery(self.path,self.authority))
        fresh=b'F'*32;data=b''.join(self.scope)+self.old
        request=(*self.scope,self.old,fresh,self.admin.sign(e.QUERY+data+fresh))
        packet,signature=self.api.withdrawal_outcome(*request)
        self.authority.public_key().verify(signature,e.QUERY+packet)
        self.assertIn(e.DRAINING.encode(),packet);self.assertTrue(packet.endswith(self.ticket.op))
        self.assertEqual(self.routes(),[(self.old,e.DRAINING)])
        self.finish();self.api.drain(*self.scope,self.old)
        packet,signature=self.api.withdrawal_outcome(*request)
        self.authority.public_key().verify(signature,e.QUERY+packet)
        self.assertIn(e.RETIRED.encode(),packet);self.assertEqual(packet[-1],0)

    def test_incomplete_child_evidence_cannot_release_barrier(self):
        self.admit();self.finish();self.retire()
        # Simulation explicite d'une trace terminale incomplète, jamais d'une
        # annulation imputée à une panne du producteur.
        with r.transaction(self.path) as db:
            db.execute('UPDATE exec_admissions SET child=NULL WHERE op=?',(self.ticket.op,))
        with self.assertRaisesRegex(r.Refus,'issue terminale incomplète'):
            self.api.drain(*self.scope,self.old)
        with self.assertRaisesRegex(r.Refus,'drainage EXEC'):self.activate()
        self.assertEqual(self.routes(),[(self.old,e.DRAINING)])

    def test_private_rpc_requires_peer_and_exact_binding(self):
        import admission_rpc
        q=self.ticket;body=b''.join(self.scope)+self.old+q.packet()+self.server.sign(e.ADMIT+q.packet())
        args=(self.path,self.authority,self.server.public_key().public_bytes_raw(),self.scope)
        for bad in (body[:-1],body+b'X',body[:32]+b'Z'*32+body[64:]):
            with self.assertRaises(r.Refus):admission_rpc.request(*args,40,bad)
        with self.assertRaises(r.Refus):
            admission_rpc.request(self.path,self.authority,self.admin.public_key().public_bytes_raw(),self.scope,40,body)
        result=admission_rpc.request(*args,40,body)
        self.authority.public_key().verify(result[-64:],e.RECEIPT+result[:-64])
        with self.assertRaises(r.Refus):admission_rpc.request(*args,40,body)

    def test_mutation_and_recovery_cannot_bypass_in_progress_state(self):
        self.retire()
        state,_=self.a.observe(*self.scope,b'F'*32)
        with self.assertRaisesRegex(r.Refus,'retrait'):
            self.a.publish(state,b'M'*16,b'Z'*32,self.server.sign(state.mutation(b'M'*16,b'Z'*32)))
        with self.assertRaisesRegex(r.Refus,'drainage EXEC'):self.activate()
        self.api.drain(*self.scope,self.old);self.activate()

if __name__=='__main__':unittest.main()
