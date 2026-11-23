"""Transitions de l'autorité hôte, distinctes des effecteurs et essais QEMU."""
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
from dataclasses import replace
import unittest
import reprise as r
import membership_retirement as m
import administration_epochs as epochs
import admission_exec as ex
import test_administration_epochs as fixture

class Membership(unittest.TestCase):
 def setUp(self):
  self.f=fixture.Epochs();self.f.setUp();self.addCleanup(self.f.doCleanups)
  m.initialize(self.f.path);self.api=m.Memberships(self.f.authority)
  self.source=m.Source(self.f.alice,b'G'*16,3)
  self.expected=((1,b'V'*32),(2,b'R'*32),(4,b'E'*32));self.op=b'M'*16
 def reserve(self,expected=None,op=None):
  args=(self.f.current(),op or self.op,b'N'*32,self.f.principal,1,self.source,expected or self.expected)
  return self.api.reserve(*args,self.f.server.sign(m.RESERVE+self.api.reservation(*args)))
 def publish(self):
  s=self.f.current();return self.api.publish(s,self.op,self.f.server.sign(m.PUBLISH+s.packet()+self.op))
 def admit(self,op=b'J'*16,source=None,kind=4,instance=b'E'*32):
  s=self.f.current();source=source or self.source;p=self.api.admission(s,source,kind,instance,op,2,b"S"*32)
  return self.api.admit(s,source,kind,instance,op,2,b"S"*32,self.f.server.sign(m.ADMIT+p))
 def finish(self,op=b'J'*16,outcome=1,evidence=b'C'*32,kind=4,instance=b'E'*32):
  s=self.f.current();p=s.packet()+op+bytes([kind])+instance+bytes([outcome])+evidence
  return self.api.finish(s,op,kind,instance,outcome,evidence,self.f.server.sign(m.FINISH+p))
 def ack(self,kind,instance):
  s=self.f.current();p=s.packet()+self.op+bytes([kind])+instance
  return self.api.acknowledge(s,self.op,kind,instance,self.f.server.sign(m.EFFECT+p))
 def query(self,fresh=b'F'*32):
  s=self.f.current();p=s.packet()+self.op+fresh
  body,sig=self.api.outcome(s,self.op,fresh,self.f.server.sign(m.QUERY+p));self.f.key.public_key().verify(sig,m.RECEIPT+body)
  self.assertEqual(len(body+sig),312);return body
 def test_reservation_survives_restart_and_does_not_publish(self):
  old=self.f.current();self.reserve();self.api=m.Memberships(r.Recovery(self.f.path,self.f.key))
  self.assertEqual(self.f.current(),old);self.assertEqual(self.query()[208:211],bytes([0,7,0]))
  with self.assertRaisesRegex(r.Refus,'consulter'):self.reserve()
  with self.assertRaisesRegex(r.Refus,'publication concurrente'):self.f.disable()
  self.assertEqual(self.f.current(),old)
 def test_no_terminal_before_real_operation_outcome_and_every_effector(self):
  self.admit();self.reserve();self.publish();self.ack(1,b'V'*32);self.ack(2,b'R'*32)
  with self.assertRaisesRegex(r.Refus,'UNCERTAIN'):self.ack(4,b'E'*32)
  self.assertEqual(self.query()[208:211],bytes([1,7,3]))
  self.finish();self.ack(4,b'E'*32);self.assertEqual(self.query()[208:211],bytes([2,7,7]))
  with self.assertRaisesRegex(r.Refus,'REVOKED'):self.admit(op=b'K'*16)
  self.admit(op=b'L'*16,source=replace(self.source,group=b'H'*16))
  state=self.f.current();args=(state,b'U'*16,b'B'*32,self.f.principal,1,bytes(16),0,epochs.ADD_MEMBER)
  self.f.api.publish(*args,self.f.server.sign(epochs.CHANGE+self.f.api.change(*args)))
  self.admit(op=b'Q'*16,source=replace(self.source,revision=8))
 def test_incomplete_inventory_and_restarted_effector_refused(self):
  self.admit()
  with self.assertRaisesRegex(r.Refus,'incomplet'):self.reserve(expected=self.expected[:2])
  self.reserve();self.publish()
  with self.assertRaisesRegex(r.Refus,'substitué'):self.ack(1,b'X'*32)
  self.assertEqual(self.query()[210],0)
 def test_lost_reply_query_never_repeats_publication_or_ack(self):
  self.reserve();self.publish();self.ack(1,b'V'*32);self.ack(2,b'R'*32);self.ack(4,b'E'*32)
  old=self.query();self.api=m.Memberships(r.Recovery(self.f.path,self.f.key));new=self.query(b'Z'*32)
  self.assertEqual(old[:216],new[:216]);self.assertNotEqual(old[216:],new[216:])
  with self.assertRaises(r.Refus):self.publish()
  with self.assertRaises(r.Refus):self.ack(1,b'V'*32)
  with r.transaction(self.f.path) as db:self.assertEqual(db.execute('SELECT count(*) FROM mutations WHERE op=?',(self.op,)).fetchone(),(1,))
 def test_concurrent_reservation_one_winner_no_second_mutation(self):
  gate=Barrier(2)
  def run(op):
   gate.wait()
   try:self.reserve(op=op);return True
   except r.Refus:return False
  with ThreadPoolExecutor(2) as pool:results=list(pool.map(run,[self.op,b'W'*16]))
  self.assertEqual(sorted(results),[False,True]);self.assertEqual(self.f.current().generation,6)
 def test_admission_concurrent_with_publication_is_ordered_or_refused(self):
  self.reserve();gate=Barrier(2)
  def publish():gate.wait();self.publish()
  def admit():
   gate.wait()
   try:self.admit();return True
   except r.Refus:return False
  with ThreadPoolExecutor(2) as pool:
   a=pool.submit(admit);p=pool.submit(publish);accepted=a.result();p.result()
  self.ack(1,b'V'*32);self.ack(2,b'R'*32)
  if accepted:
   with self.assertRaisesRegex(r.Refus,'UNCERTAIN'):self.ack(4,b'E'*32)
   self.finish(outcome=2,evidence=bytes(32))
  self.ack(4,b'E'*32);self.assertEqual(self.query()[208],2)
 def test_signature_scope_and_unknown_result_are_not_success(self):
  args=(self.f.current(),self.op,b'N'*32,self.f.principal,1,self.source,self.expected)
  with self.assertRaises(r.Refus):self.api.reserve(*args,self.f.admin.sign(m.RESERVE+self.api.reservation(*args)))
  with self.assertRaises(r.Refus):self.query()
  self.reserve();self.publish()
  with self.assertRaises(r.Refus):self.finish()
  self.assertEqual(self.query()[208],1)
 def test_classes_need_all_instances_and_storage_publication(self):
  self.expected=((1,b'V'*32),(1,b'W'*32),(2,b'R'*32),(4,b'E'*32))
  self.reserve();self.publish();self.ack(1,b'V'*32);self.ack(2,b'R'*32);self.ack(4,b'E'*32)
  self.assertEqual(self.query()[208:211],bytes([1,7,6]));self.ack(1,b'W'*32)
  self.assertEqual(self.query()[208:211],bytes([2,7,7]))
 def test_limits_leave_no_partial_reservation(self):
  for expected in [(),((1,b'V'*32),)*2,tuple((1,bytes([i])*32) for i in range(1,34))]:
   args=(self.f.current(),self.op,b'N'*32,self.f.principal,1,self.source,expected)
   with self.assertRaises(r.Refus):self.api.reservation(*args)
  with r.transaction(self.f.path) as db:self.assertEqual(db.execute('SELECT count(*) FROM membership_changes').fetchone(),(0,))
 def test_restart_discovers_reserved_origin_without_repeating_mutation(self):
  self.reserve();self.api=m.Memberships(r.Recovery(self.f.path,self.f.key))
  state=self.f.current();fresh=b'Z'*32;packet=state.packet()+fresh
  body,sig=self.api.pending(state,fresh,self.f.server.sign(m.PENDING+packet))
  self.f.key.public_key().verify(sig,m.PENDING+body)
  self.assertEqual(body[:144],packet);self.assertEqual(body[144],1)
  self.assertEqual(body[145:161],self.op);self.assertEqual(body[-1],0)
 def test_retired_subject_epoch_cannot_admit_group_operation(self):
  self.f.disable()
  with self.assertRaisesRegex(r.Refus,'époque'):self.admit()
 def test_reservation_identifier_cannot_be_reused_by_exec(self):
  self.reserve()
  request=replace(self.f.ticket,op=self.op)
  with self.assertRaises(r.Refus):self.f.admit(request)
 def test_incarnation_drain_does_not_block_reconciliation_but_blocks_activation(self):
  self.admit();self.reserve();self.publish()
  packet=b''.join(self.f.scope)+self.f.incarnation
  self.f.exec.begin_withdrawal(*self.f.scope,self.f.incarnation,self.f.admin.sign(ex.WITHDRAW+packet))
  with r.transaction(self.f.path) as db:
   with self.assertRaisesRegex(r.Refus,'UNCERTAIN'):m.guard_activation(db,*self.f.scope)
  self.finish();self.ack(1,b'V'*32);self.ack(2,b'R'*32);self.ack(4,b'E'*32)
  self.assertEqual(self.query()[208],2)
  with r.transaction(self.f.path) as db:m.guard_activation(db,*self.f.scope)
 def test_opt_in_without_schema_is_refused_without_implicit_migration(self):
  import membership_rpc as rpc
  with r.transaction(self.f.path) as db:
   db.execute('DROP TABLE membership_effectors');db.execute('DROP TABLE membership_operations');db.execute('DROP TABLE membership_changes')
  old=self.f.current()
  with self.assertRaisesRegex(r.Refus,'non initialisé'):
   rpc.request(self.f.path,self.f.key,self.f.server.public_key().public_bytes_raw(),self.f.scope,54,bytes(128),membership=True)
  self.assertEqual(self.f.current(),old)
  with r.transaction(self.f.path) as db:self.assertFalse(m.available(db))
 def test_private_rpc_scope_enablement_and_signed_receipt(self):
  import membership_rpc as rpc
  s=self.f.current();args=(s,self.op,b'N'*32,self.f.principal,1,self.source,self.expected)
  q=self.api.reservation(*args);body=b''.join(self.f.scope)+s.instance+q+self.f.server.sign(m.RESERVE+q)
  peer=self.f.server.public_key().public_bytes_raw()
  with self.assertRaisesRegex(r.Refus,'non activée'):rpc.request(self.f.path,self.f.key,peer,self.f.scope,54,body)
  self.assertEqual(rpc.request(self.f.path,self.f.key,peer,self.f.scope,54,body,membership=True),b'')
  fresh=b'Z'*32;q=s.packet()+self.op+fresh
  body=b''.join(self.f.scope)+s.instance+q+self.f.server.sign(m.QUERY+q)
  result=rpc.request(self.f.path,self.f.key,peer,self.f.scope,59,body,membership=True)
  self.f.key.public_key().verify(result[248:],m.RECEIPT+result[:248]);self.assertEqual(result[208],0)
  for bad in [body[:-1],body+bytes(1),bytes(1)+body[1:]]:
   with self.assertRaises(r.Refus):rpc.request(self.f.path,self.f.key,peer,self.f.scope,59,bad,membership=True)
 def test_lost_finish_reply_can_be_reconciled_without_second_operation(self):
  self.admit();self.reserve();self.publish();self.finish()
  self.api=m.Memberships(r.Recovery(self.f.path,self.f.key));s=self.f.current();op=b'J'*16;fresh=b'Z'*32
  q=s.packet()+op+fresh
  body,sig=self.api.operation_outcome(s,op,fresh,self.f.server.sign(m.QUERY+q))
  self.f.key.public_key().verify(sig,m.QUERY+body)
  self.assertEqual(body[:8],b'NAMBOU01');self.assertEqual(body[8:168],q)
  self.assertEqual(body[417],1);self.assertEqual(body[418:450],b'C'*32)
  with self.assertRaises(r.Refus):self.finish()
  with r.transaction(self.f.path) as db:
   self.assertEqual(db.execute('SELECT count(*) FROM membership_operations').fetchone(),(1,))
 def test_invalid_terminal_cannot_drain(self):
  self.admit();self.reserve();self.publish()
  for outcome,evidence in [(0,bytes(32)),(1,bytes(32)),(2,b'C'*32)]:
   with self.assertRaises(r.Refus):self.finish(outcome=outcome,evidence=evidence)
  self.assertEqual(self.query()[208],1)

 def test_incarnation_cannot_retire_with_individual_group_operation_pending(self):
  self.admit()
  packet=b''.join(self.f.scope)+self.f.incarnation
  self.f.exec.begin_withdrawal(*self.f.scope,self.f.incarnation,self.f.admin.sign(ex.WITHDRAW+packet))
  with self.assertRaisesRegex(r.Refus,'groupe'):self.f.exec.drain(*self.f.scope,self.f.incarnation)
  with r.transaction(self.f.path) as db:
   self.assertEqual(db.execute('SELECT state FROM exec_routes').fetchone(),(ex.DRAINING,))
  self.finish(outcome=2,evidence=bytes(32))
  self.f.exec.drain(*self.f.scope,self.f.incarnation)
 def test_subject_cannot_retire_until_group_operation_is_reconciled(self):
  self.admit();self.f.disable();self.f.ack()
  with self.assertRaisesRegex(r.Refus,'groupe'):self.f.api.drain(*self.f.scope,b'R'*16)
  with self.assertRaises(r.Refus):self.f.enable()
  self.finish();self.f.api.drain(*self.f.scope,b'R'*16);self.f.enable()
  with self.assertRaisesRegex(r.Refus,'époque'):self.admit(op=b'K'*16)

 def test_pending_limit_reuses_terminal_slot_without_duplicate_admission(self):
  for i in range(1,65):self.admit(op=i.to_bytes(16,'big'))
  with self.assertRaisesRegex(r.Refus,'LIMIT'):self.admit(op=(65).to_bytes(16,'big'))
  with r.transaction(self.f.path) as db:
   self.assertEqual(db.execute('SELECT count(*) FROM membership_operations').fetchone(),(64,))
  self.finish(op=(1).to_bytes(16,'big'),outcome=2,evidence=bytes(32))
  self.admit(op=(65).to_bytes(16,'big'))
  with r.transaction(self.f.path) as db:
   self.assertEqual(db.execute('SELECT count(*) FROM membership_operations WHERE state=0').fetchone(),(64,))
  with self.assertRaises(r.Refus):self.admit(op=(1).to_bytes(16,'big'))

 def test_rpc_finish_advances_subject_drain_only_after_signed_terminal_fact(self):
  import membership_rpc as rpc
  self.admit();self.f.disable();self.f.ack()
  with self.assertRaisesRegex(r.Refus,'groupe'):self.f.api.drain(*self.f.scope,b'R'*16)
  state=self.f.current();q=state.packet()+b'J'*16+bytes([4])+b'E'*32+bytes([1])+b'C'*32
  body=b''.join(self.f.scope)+state.instance+q+self.f.server.sign(m.FINISH+q)
  peer=self.f.server.public_key().public_bytes_raw()
  self.assertEqual(rpc.request(self.f.path,self.f.key,peer,self.f.scope,rpc.FINISH,body,membership=True),b'')
  with r.transaction(self.f.path) as db:
   self.assertIsNotNone(db.execute('SELECT terminal FROM admin_changes WHERE op=?',(b'R'*16,)).fetchone()[0])
   self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='subject-retired'").fetchone(),(1,))
  self.f.enable()

if __name__=='__main__':unittest.main()
