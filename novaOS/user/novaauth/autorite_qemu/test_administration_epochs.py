"""Composant hôte R2. Ni route NXADMIN1 ni effecteur VFS invité qualifiés ici."""
from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
from pathlib import Path
from tempfile import TemporaryDirectory
from threading import Barrier
import unittest
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
import reprise as r
import admission_exec as e
import administration_epochs as a

class Epochs(unittest.TestCase):
 def setUp(self):
  self.tmp=TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.path=Path(self.tmp.name)/'authority.db'
  self.server,self.admin,self.key=[Ed25519PrivateKey.generate() for _ in range(3)]
  r.initialize(self.path,self.key.public_key().public_bytes_raw());e.initialize(self.path);a.initialize(self.path)
  self.authority=r.Recovery(self.path,self.key);self.api=a.Epochs(self.authority);self.exec=e.Admissions(self.authority)
  self.scope=(b'S'*16,b'D'*16);self.incarnation=b'I'*32;self.principal=b'P'*16;self.alice=b'A'*16
  self.authority.provision(*self.scope,self.incarnation,6,b'H'*32,
   self.server.public_key().public_bytes_raw(),self.admin.public_key().public_bytes_raw())
  self.exec.enroll(*self.scope,self.incarnation,self.admin.sign(e.ENROLL+b''.join(self.scope)+self.incarnation))
  state=self.current();subjects=[(self.principal,1),(self.alice,2)]
  self.api.enroll(state,subjects,self.admin.sign(a.ENROLL+self.api.enrollment(state,subjects)))
  self.ticket=a.SubjectAdmission(*self.scope,self.incarnation,6,b'H'*32,b'O'*32,b'L'*32,
   b'X'*16,b'E'*32,b'T'*16,b'Q'*16,self.alice,2)
 def current(self):
  with r.transaction(self.path) as db:return self.authority._current(db,*self.scope)
 def admit(self,q=None):
  q=q or self.ticket;return self.exec.admit(q,self.server.sign(e.ADMIT+q.packet()))
 def disable(self):
  state=self.current();args=(state,b'R'*16,b'N'*32,self.principal,1,self.alice,2,a.DISABLE)
  return self.api.publish(*args,self.server.sign(a.CHANGE+self.api.change(*args)))
 def ack(self,mask=7):
  s=self.current();p=s.packet()+b'R'*16+bytes([mask])
  return self.api.acknowledge(s,b'R'*16,mask,self.server.sign(a.EFFECTS+p))
 def finish(self,q=None,state=e.CREATED,child=b'C'*32):
  q=q or self.ticket;return self.exec.finish(q,state,child,self.server.sign(e.FINISH+q.finish(state,child)))
 def enable(self):
  s=self.current();args=(s,b'F'*16,b'B'*32,self.principal,1,self.alice,2,a.REACTIVATE)
  return self.api.publish(*args,self.server.sign(a.CHANGE+self.api.change(*args)))
 def query(self):
  s=self.current();p=s.packet()+b'R'*16+b'Z'*32
  out,sig=self.api.outcome(s,b'R'*16,b'Z'*32,self.server.sign(a.QUERY+p))
  self.key.public_key().verify(sig,a.QUERY+out);return out
 def test_no_terminal_receipt_until_effectors_and_launch_drained(self):
  self.admit();self.disable()
  with self.assertRaisesRegex(r.Refus,'clôture'):self.enable()
  with self.assertRaisesRegex(r.Refus,'effecteurs'):self.api.drain(*self.scope,b'R'*16)
  self.ack(1);self.ack(2)
  with self.assertRaisesRegex(r.Refus,'effecteurs'):self.api.drain(*self.scope,b'R'*16)
  self.ack(4)
  with self.assertRaisesRegex(r.Refus,'lancement'):self.api.drain(*self.scope,b'R'*16)
  self.finish();body,sig=self.api.drain(*self.scope,b'R'*16)
  self.key.public_key().verify(sig,a.RETIRED+body)
  self.assertEqual(len(body),128);self.assertEqual(body[88:96],b'\x01'+bytes(7))
  self.assertEqual(body[96:],b'H'*32);self.assertEqual(int.from_bytes(body[72:80],'big'),7)
  self.enable();s=self.current()
  old=replace(self.ticket,generation=s.generation,head=s.head,op=b'W'*16,ticket=b'V'*16)
  with self.assertRaisesRegex(r.Refus,'époque'):self.admit(old)
  new=replace(old,authentication_epoch=s.generation,session=b'J'*32)
  self.admit(new)
 def test_lost_responses_are_queried_without_new_mutation(self):
  self.disable();initial=self.query();self.ack();body,sig=self.api.drain(*self.scope,b'R'*16)
  self.api=a.Epochs(r.Recovery(self.path,self.key));terminal=self.query()
  self.assertNotEqual(initial,terminal);self.assertTrue(terminal.endswith(body+sig))
  self.assertEqual(terminal,self.query())
  with self.assertRaisesRegex(r.Refus,'terminé'):self.api.drain(*self.scope,b'R'*16)
  self.enable()
 def test_cancelled_tombstone_and_terminal_evidence(self):
  self.admit();self.disable();self.ack()
  with self.assertRaises(r.Refus):self.api.drain(*self.scope,b'R'*16)
  self.finish(state=e.CANCELLED,child=bytes(32))
  self.api.drain(*self.scope,b'R'*16)
  with self.assertRaises(r.Refus):self.admit()
  self.enable()
 def test_admission_concurrent_with_disable_has_one_order(self):
  start=Barrier(2)
  def admit():
   start.wait(timeout=5)
   try:self.admit();return True
   except r.Refus:return False
  def disable():start.wait(timeout=5);self.disable()
  with ThreadPoolExecutor(2) as pool:
   f=pool.submit(admit);g=pool.submit(disable);accepted=f.result();g.result()
  self.ack()
  if accepted:
   with self.assertRaisesRegex(r.Refus,'lancement'):self.api.drain(*self.scope,b'R'*16)
   self.finish()
  self.api.drain(*self.scope,b'R'*16)
  with self.assertRaises(r.Refus):self.admit()
 def test_concurrent_consumers_only_one_scoped_admission(self):
  start=Barrier(2)
  def run(_):
   start.wait(timeout=5)
   try:self.admit();return 'OK'
   except r.Refus:return 'REFUSED'
  with ThreadPoolExecutor(2) as p:self.assertCountEqual(list(p.map(run,range(2))),['OK','REFUSED'])
  with r.transaction(self.path) as db:
   self.assertEqual(db.execute('SELECT count(*) FROM subject_admissions').fetchone(),(1,))
 def test_legacy_downgrade_refused_only_in_enrolled_profile(self):
  legacy=e.Admission(**{k:getattr(self.ticket,k) for k in e.Admission.__dataclass_fields__})
  with self.assertRaisesRegex(r.Refus,'époque'):self.admit(legacy)
  state=self.current();packet=state.mutation(b'M'*16,b'Z'*32)
  with self.assertRaisesRegex(r.Refus,'administratif'):self.authority.publish(state,b'M'*16,b'Z'*32,self.server.sign(packet))
  self.assertEqual(state,self.current())
 def test_replayed_signature_and_changed_epoch_rejected(self):
  q=self.ticket;signature=self.server.sign(e.ADMIT+q.packet())
  for changed in [replace(q,subject=self.principal),replace(q,authentication_epoch=3),replace(q,session=b'K'*32)]:
   with self.assertRaises(r.Refus):self.exec.admit(changed,signature)
  self.assertEqual(a.SubjectAdmission.decode(q.packet()),q)
  for malformed in [q.packet()[:-1],q.packet()+b'0',b'NAEXEC01'+q.packet()[8:]]:
   with self.assertRaises(r.Refus):a.SubjectAdmission.decode(malformed)
 def test_incomplete_terminal_and_unknown_effects_cannot_release_barrier(self):
  self.admit();self.disable();self.ack();self.finish()
  with r.transaction(self.path) as db:db.execute('UPDATE exec_admissions SET child=NULL WHERE op=?',(self.ticket.op,))
  with self.assertRaisesRegex(r.Refus,'lancement'):self.api.drain(*self.scope,b'R'*16)
  with self.assertRaises(r.Refus):self.ack(8)
 def test_missing_link_is_not_an_empty_drain(self):
  self.admit();self.disable();self.ack()
  with r.transaction(self.path) as db:db.execute('DELETE FROM subject_admissions WHERE op=?',(self.ticket.op,))
  with self.assertRaisesRegex(r.Refus,'filiation'):self.api.drain(*self.scope,b'R'*16)
 def test_old_route_cannot_acknowledge_or_reactivate(self):
  self.disable();s=self.current();wrong=replace(s,instance=b'Z'*32)
  packet=wrong.packet()+b'R'*16+b'\x07'
  with self.assertRaisesRegex(r.Refus,'obsolète'):self.api.acknowledge(wrong,b'R'*16,7,self.server.sign(a.EFFECTS+packet))
  with self.assertRaisesRegex(r.Refus,'effecteurs'):self.api.drain(*self.scope,b'R'*16)
 def test_atomic_rollback_if_metadata_rejects_change(self):
  s=self.current();args=(s,b'R'*16,b'N'*32,self.principal,1,self.alice,5,a.DISABLE)
  with self.assertRaises(r.Refus):self.api.publish(*args,self.server.sign(a.CHANGE+self.api.change(*args)))
  self.assertEqual(s,self.current())
  with r.transaction(self.path) as db:
   self.assertEqual(db.execute('SELECT count(*) FROM mutations').fetchone(),(0,))
   self.assertEqual(db.execute('SELECT count(*) FROM admin_changes').fetchone(),(0,))

if __name__=='__main__':unittest.main()
