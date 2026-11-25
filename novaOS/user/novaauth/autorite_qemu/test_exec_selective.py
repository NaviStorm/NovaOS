"""Autorité hôte uniquement ; aucune qualification du raccord invité déduite."""
from dataclasses import replace
from hashlib import sha256
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
import unittest
import reprise as r
import admission_exec as ex
import exec_descriptor as w
import exec_selective as s
import test_membership_retirement as fixture

class Selective(unittest.TestCase):
 def setUp(self):
  self.m=fixture.Membership();self.m.setUp();self.addCleanup(self.m.doCleanups);self.f=self.m.f
  s.initialize(self.f.path);self.api=s.Selective(self.f.authority)
 def descriptor(self,op=b'Q'*16,groups=()):
  q=replace(self.f.ticket,op=op)
  v=dict(server=q.server,domain=q.domain,incarnation=q.incarnation,owner=q.owner,
   effector=b'E'*32,subject=q.subject,authentication_epoch=q.authentication_epoch,
   session=q.session,route=b'R'*32,operation=op,installation=q.installation,
   recipient=q.subject,package=b'P'*16,version=1,machine=62,role=1,image=q.image,
   volume=b'V'*32,object=11,object_revision=2,offset=64,image_size=1024,
   manifest=b'M'*32,generation=q.generation,head=q.head,deadline=12345,clock=b'C'*32)
  ds=[w.Dependency(1,1,0,q.subject,11,bytes(16),0,0,2)]
  ds += [w.Dependency(2,2,1,q.subject,12+i,g,3,2,2) for i,g in enumerate(groups)]
  return w.Descriptor.encode(v,ds)
 def register(self,d):return self.api.register(d,self.f.server.sign(w.REGISTER+d.body))
 def request(self,d):
  current=self.f.current()
  return replace(self.f.ticket,op=d.operation,ticket=d.operation,generation=current.generation,head=current.head)
 def admit(self,d,q=None):
  q=q or self.request(d);digest=sha256(d.body).digest()
  return self.api.admit(q,digest,self.f.server.sign(w.ADMIT+q.packet()+digest))
 def finish(self,d,state=1):
  with r.transaction(self.f.path) as db:packet=db.execute('SELECT packet FROM exec_admissions WHERE op=?',(d.operation,)).fetchone()[0]
  import administration_epochs as ep
  q=ep.SubjectAdmission.decode(packet);child=b'C'*32 if state==1 else bytes(32)
  return self.f.exec.finish(q,state,child,self.f.server.sign(ex.FINISH+q.finish(state,child)))
 def test_direct_h_survive_g_mutation_and_g_does_not(self):
  ds=[self.descriptor(b'D'*16),self.descriptor(b'G'*16,(b'G'*16,)),self.descriptor(b'H'*16,(b'H'*16,))]
  for d in ds:self.register(d)
  self.m.reserve();self.m.publish()
  for d in (ds[0],ds[2]):self.admit(d);self.finish(d)
  with self.assertRaisesRegex(r.Refus,'REVOKED'):self.admit(ds[1])
  for k,i in self.m.expected:self.m.ack(k,i)
  self.assertEqual(self.m.query()[208:211],bytes((2,7,7)))
  with r.transaction(self.f.path) as db:self.assertEqual(db.execute('SELECT count(*) FROM exec_admissions').fetchone(),(2,))
 def test_multiple_dependencies_single_admission_and_real_drainage(self):
  d=self.descriptor(groups=(b'G'*16,b'H'*16));self.register(d);self.admit(d)
  self.m.reserve();self.m.publish();self.m.ack(1,b'V'*32);self.m.ack(2,b'R'*32)
  with self.assertRaisesRegex(r.Refus,'UNCERTAIN'):self.m.ack(4,b'E'*32)
  self.finish(d);self.m.ack(4,b'E'*32)
  with r.transaction(self.f.path) as db:
   self.assertEqual(db.execute('SELECT count(*) FROM exec_admissions').fetchone(),(1,))
   self.assertEqual(db.execute('SELECT count(*) FROM exec_dependencies').fetchone(),(2,))
   self.assertEqual(db.execute('SELECT count(*) FROM membership_operations').fetchone(),(0,))
 def test_reservation_blocks_new_group_admission_and_registration(self):
  d=self.descriptor(groups=(b'G'*16,));self.register(d);self.m.reserve()
  with self.assertRaisesRegex(r.Refus,'REVOKED'):self.admit(d)
  with self.assertRaisesRegex(r.Refus,'REVOKED'):self.register(self.descriptor(b'Z'*16,(b'G'*16,)))
  self.assertEqual(self.m.query()[208:211],bytes((0,7,0)))
 def test_inventory_includes_dormant_descriptor(self):
  self.register(self.descriptor(groups=(b'G'*16,)))
  with self.assertRaisesRegex(r.Refus,'incomplet'):self.m.reserve(expected=self.m.expected[:2])
 def test_legacy_admission_cannot_bypass_descriptor(self):
  d=self.descriptor();self.register(d);q=self.request(d)
  with self.assertRaisesRegex(r.Refus,'identifiant'):self.f.exec.admit(q,self.f.server.sign(ex.ADMIT+q.packet()))
 def test_signature_and_every_binding_refused_without_admission(self):
  d=self.descriptor();self.register(d);q=self.request(d);digest=sha256(d.body).digest()
  with self.assertRaises(r.Refus):self.api.admit(q,digest,self.f.admin.sign(w.ADMIT+q.packet()+digest))
  for field,value in [('owner',b'Z'*32),('session',b'Z'*32),('installation',b'Z'*16),('image',b'Z'*32),('subject',b'Z'*16),('authentication_epoch',3),('incarnation',b'Z'*32),('domain',b'Z'*16),('server',b'Z'*16)]:
   with self.subTest(field=field),self.assertRaisesRegex(r.Refus,'substituée'):self.admit(d,replace(q,**{field:value}))
  with self.assertRaises(r.Refus):self.api.admit(q,b'Z'*32,self.f.server.sign(w.ADMIT+q.packet()+b'Z'*32))
  with r.transaction(self.f.path) as db:self.assertEqual(db.execute('SELECT count(*) FROM exec_admissions').fetchone(),(0,))
 def test_fresh_head_compare_is_required(self):
  d=self.descriptor();self.register(d);q=self.request(d);self.m.reserve();self.m.publish()
  with self.assertRaisesRegex(r.Refus,'CONFLICT'):self.admit(d,q)
  self.admit(d);self.finish(d)
 def test_lost_replies_query_after_restart_without_second_mutation(self):
  d=self.descriptor();self.register(d);self.api=s.Selective(r.Recovery(self.f.path,self.f.key))
  data=d.server+d.domain+d.incarnation+d.operation+b'F'*32
  record,sig=self.api.lookup(d.server,d.domain,d.incarnation,d.operation,b'F'*32,self.f.server.sign(w.LOOKUP+data))
  self.f.key.public_key().verify(sig,w.LOOKUP+record);self.assertEqual(record[120:-64],d.body)
  with self.assertRaises(r.Refus):self.register(d)
  self.admit(d);q=self.request(d);self.finish(d);before=None
  for fresh in (b'A'*32,b'B'*32):
   reply,sig=self.f.exec.query(q,fresh,self.f.server.sign(ex.QUERY+q.packet()+fresh))
   self.f.key.public_key().verify(sig,ex.QUERY+reply)
   if before is not None:self.assertEqual(reply[-49:],before[-49:])
   before=reply
  with self.assertRaises(r.Refus):self.admit(d)
  with r.transaction(self.f.path) as db:
   self.assertEqual(db.execute('SELECT count(*) FROM exec_admissions').fetchone(),(1,))
   self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='created'").fetchone(),(1,))
 def test_cancel_tombstone_blocks_delayed_admission(self):
  d=self.descriptor();self.register(d);q=self.request(d);digest=sha256(d.body).digest()
  proof=self.f.server.sign(ex.FINISH+q.finish(2,bytes(32)));self.api.cancel(q,digest,proof)
  with self.assertRaises(r.Refus):self.admit(d)
  with self.assertRaises(r.Refus):self.api.cancel(q,digest,proof)
  with r.transaction(self.f.path) as db:self.assertEqual(db.execute('SELECT state,child FROM exec_admissions').fetchone(),(2,bytes(32)))
 def test_tombstone_refusal_and_lost_queries_do_not_consume_the_first_finish(self):
  descriptor=self.descriptor(groups=(b'G'*16,))
  self.register(descriptor)
  request=self.request(descriptor)
  self.admit(descriptor,request)
  self.m.reserve()
  self.m.publish()
  for kind,instance in self.m.expected[:2]:self.m.ack(kind,instance)
  proof=self.f.server.sign(ex.FINISH+request.finish(2,bytes(32)))
  with self.assertRaises(r.Refus):
   self.api.cancel(request,sha256(descriptor.body).digest(),proof)
  with self.assertRaisesRegex(r.Refus,'UNCERTAIN'):self.m.ack(4,b'E'*32)

  def query(challenge):
   reply,signature=self.f.exec.query(request,challenge,self.f.server.sign(ex.QUERY+request.packet()+challenge))
   self.f.key.public_key().verify(signature,ex.QUERY+reply)
   self.assertEqual(reply[:len(request.packet())],request.packet())
   return reply[-49:]

  unused_lost_reply = query(b'A'*32)
  self.assertEqual(unused_lost_reply[8],0)
  pending = query(b'B'*32)
  self.assertEqual(pending,unused_lost_reply)
  self.finish(descriptor,2)
  # Même en l'absence de réponse FINISH, les consultations seules suffisent.
  terminal = query(b'C'*32)
  self.assertEqual(terminal,query(b'D'*32))
  self.assertEqual(terminal[8:41],bytes([2])+bytes(32))
  self.m.ack(4,b'E'*32)
  self.assertEqual(self.m.query()[208:211],bytes((2,7,7)))
  with r.transaction(self.f.path) as db:
   self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='cancelled'").fetchone(),(1,))
   self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='created'").fetchone(),(0,))

 def test_two_consumers_one_admission(self):
  d=self.descriptor();self.register(d);barrier=Barrier(2)
  def run():
   barrier.wait()
   try:self.admit(d);return True
   except r.Refus:return False
  with ThreadPoolExecutor(2) as pool:result=list(pool.map(lambda _:run(),range(2)))
  self.assertEqual(sorted(result),[False,True])
 def test_retirement_concurrent_with_admission_has_durable_order(self):
  d=self.descriptor(groups=(b'G'*16,));self.register(d);barrier=Barrier(2)
  def run():
   barrier.wait()
   try:self.admit(d);return True
   except r.Refus:return False
  def reserve():barrier.wait();return self.m.reserve()
  with ThreadPoolExecutor(2) as pool:
   a=pool.submit(run);b=pool.submit(reserve);accepted=a.result();position=b.result()
  self.m.publish();self.m.ack(1,b'V'*32);self.m.ack(2,b'R'*32)
  if accepted:
   with self.assertRaisesRegex(r.Refus,'UNCERTAIN'):self.m.ack(4,b'E'*32)
   with r.transaction(self.f.path) as db:self.assertLess(db.execute('SELECT sequence FROM exec_admissions').fetchone()[0],position)
   self.finish(d,2)
  self.m.ack(4,b'E'*32)
 def test_canonical_sizes_versions_and_padding(self):
  d=self.descriptor(groups=(b'G'*16,b'H'*16,b'I'*16));self.assertEqual(len(d.body),800)
  for data in (d.body[:-1],d.body+b'\0',b'NXEXDP02'+d.body[8:],d.body[:504]+b'X'+d.body[505:]):
   with self.assertRaises(r.Refus):w.Descriptor(data)
  for at in (295,512,513,515):
   p=bytearray(d.body);p[at]=255
   with self.assertRaises(r.Refus):w.Descriptor(bytes(p))
  with self.assertRaises(r.Refus):self.descriptor(groups=(b'G'*16,b'H'*16,b'I'*16,b'J'*16))
 def test_duplicate_and_direct_group_confusion_refused(self):
  d=self.descriptor(groups=(b'G'*16,));p=bytearray(d.body);p[295]=3;p+=p[-72:]
  with self.assertRaisesRegex(r.Refus,'dupliquées'):w.Descriptor(bytes(p))
  for at,value in ((513,2),(552,1)):
   p=bytearray(d.body);p[at]=value
   with self.assertRaises(r.Refus):w.Descriptor(bytes(p))
 def test_id_reserved_against_other_mutations(self):
  d=self.descriptor();self.register(d)
  with r.transaction(self.f.path) as db:self.assertTrue(self.f.authority._decided(db,d.operation))
 def test_quota_no_silent_purge(self):
  for i in range(s.MAX_DESCRIPTORS):self.register(self.descriptor((i+1).to_bytes(16,'big')))
  with self.assertRaisesRegex(r.Refus,'LIMIT'):self.register(self.descriptor(b'Z'*16))
  with r.transaction(self.f.path) as db:self.assertEqual(db.execute('SELECT count(*) FROM exec_descriptors').fetchone(),(s.MAX_DESCRIPTORS,))

 def test_rpc_is_opt_in_and_verifies_the_authenticated_outer_scope(self):
  import exec_selective_rpc as rpc
  d=self.descriptor();peer=self.f.server.public_key().public_bytes_raw()
  body=d.server+d.domain+d.incarnation+d.body+self.f.server.sign(w.REGISTER+d.body)
  args=(self.f.path,self.f.key,peer,self.f.scope,rpc.REGISTER,body)
  with self.assertRaises(r.Refus):rpc.request(*args)
  with self.assertRaises(r.Refus):rpc.request(*args[:-1],b'X'+body[1:],selective=True)
  receipt=rpc.request(*args,selective=True)
  self.f.key.public_key().verify(receipt[-64:],w.RECEIPT+receipt[:-64])
  q=self.request(d);digest=sha256(d.body).digest();payload=q.packet()+digest
  proof=self.f.server.sign(w.ADMIT+payload)
  admitted=rpc.request(self.f.path,self.f.key,peer,self.f.scope,rpc.ADMIT,body[:64]+payload+proof,selective=True)
  self.f.key.public_key().verify(admitted[-64:],ex.RECEIPT+admitted[:-64])
 def test_native_refusal_codes_preserve_the_first_cause(self):
  d=self.descriptor(groups=(b'G'*16,));self.register(d);old=self.request(d);self.m.reserve()
  with self.assertRaises(s.Refusal) as caught:self.admit(d)
  self.assertEqual(caught.exception.status,10)
  self.m.publish()
  with self.assertRaises(s.Refusal) as caught:self.admit(d,old)
  self.assertEqual(caught.exception.status,24)
 def test_python_rust_fixture_matches_exactly(self):
  from pathlib import Path
  self.assertEqual(self.descriptor(groups=(b'G'*16,b'H'*16,b'I'*16)).body,
   (Path(__file__).parent.parent/'recuperation/tests/fixtures/exec_descriptor_v1.bin').read_bytes())

if __name__=='__main__':unittest.main()
