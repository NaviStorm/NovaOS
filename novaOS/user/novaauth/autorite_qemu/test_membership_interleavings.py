"""Q2/Q3/Q4 : entrelacements déterministes de l'autorité, pas des invités."""
from dataclasses import replace
import unittest
import reprise as r
import membership_retirement as m
import test_membership_retirement as fixture

class Interleavings(unittest.TestCase):
 def fresh(self):
  f=fixture.Membership();f.setUp();self.addCleanup(f.doCleanups);return f
 def test_three_admission_orders_keep_h_and_require_individual_g_outcome(self):
  for phase in ('avant-reservation','entre-reservation-publication','apres-publication'):
   with self.subTest(phase=phase):
    f=self.fresh();op=b'V'*16
    if phase=='avant-reservation':f.admit(op=op,kind=1,instance=b'V'*32)
    f.reserve()
    if phase=='entre-reservation-publication':f.admit(op=op,kind=1,instance=b'V'*32)
    f.publish()
    if phase=='apres-publication':
     with self.assertRaisesRegex(r.Refus,'REVOKED'):f.admit(op=op,kind=1,instance=b'V'*32)
    # H reste admissible, mais n'est pas un remplacement de l'ancienne G.
    f.admit(op=b'H'*16,source=replace(f.source,group=b'H'*16),kind=1,instance=b'V'*32)
    f.ack(2,b'R'*32);f.ack(4,b'E'*32)
    if phase!='apres-publication':
     with self.assertRaisesRegex(r.Refus,'UNCERTAIN'):f.ack(1,b'V'*32)
     f.finish(op=op,kind=1,instance=b'V'*32,outcome=2,evidence=bytes(32))
    f.ack(1,b'V'*32)
    self.assertEqual(f.query()[208:211],bytes([2,7,7]))
    # L'opération H demeure en vol ; le terminal G ne l'a pas annulée.
    with r.transaction(f.f.path) as db:
     rows=db.execute('SELECT op,state FROM membership_operations ORDER BY sequence').fetchall()
     self.assertEqual(rows[-1],(b'H'*16,0))
     self.assertEqual(len(rows),1 if phase=='apres-publication' else 2)
    f.finish(op=b'H'*16,kind=1,instance=b'V'*32)
 def test_mixed_outcomes_survive_restart_without_fabricating_missing_finish(self):
  f=self.fresh()
  for op in (b'A'*16,b'B'*16):f.admit(op=op,kind=1,instance=b'V'*32)
  f.reserve();f.publish();f.finish(op=b'A'*16,kind=1,instance=b'V'*32)
  f.api=m.Memberships(r.Recovery(f.f.path,f.f.key))
  f.ack(2,b'R'*32);f.ack(4,b'E'*32)
  with self.assertRaisesRegex(r.Refus,'UNCERTAIN'):f.ack(1,b'V'*32)
  self.assertEqual(f.query()[208:211],bytes([1,7,6]))
  f.finish(op=b'B'*16,kind=1,instance=b'V'*32,outcome=2,evidence=bytes(32))
  f.api=m.Memberships(r.Recovery(f.f.path,f.f.key));f.ack(1,b'V'*32)
  with r.transaction(f.f.path) as db:
   rows=db.execute('SELECT op,state,evidence,sequence,terminal_sequence FROM membership_operations ORDER BY sequence').fetchall()
   self.assertEqual([(op,state,evidence) for op,state,evidence,_,_ in rows],[(b'A'*16,1,b'C'*32),(b'B'*16,2,bytes(32))])
   self.assertTrue(all(end>start for _,_,_,start,end in rows))
   events=db.execute("SELECT kind,count(*) FROM exec_events WHERE kind IN ('membership-operation-finished','membership-retired') GROUP BY kind").fetchall()
   self.assertEqual(events,[('membership-operation-finished',2),('membership-retired',1)])
  before=f.query()
  with self.assertRaises(r.Refus):f.finish(op=b'B'*16,kind=1,instance=b'V'*32,outcome=2,evidence=bytes(32))
  self.assertEqual(f.query(b'Z'*32)[:216],before[:216])
 def test_pending_reservation_and_partial_ack_both_block_activation_after_restart(self):
  for stage in ('reserve','published','partial-ack'):
   with self.subTest(stage=stage):
    f=self.fresh();f.reserve()
    if stage!='reserve':f.publish()
    if stage=='partial-ack':f.ack(1,b'V'*32)
    before=f.query();f.api=m.Memberships(r.Recovery(f.f.path,f.f.key))
    after=f.query(b'Z'*32);self.assertEqual(before[:216],after[:216])
    with r.transaction(f.f.path) as db:
     with self.assertRaisesRegex(r.Refus,'UNCERTAIN'):m.guard_activation(db,*f.f.scope)
     self.assertEqual(db.execute('SELECT count(*) FROM mutations').fetchone()[0],0 if stage=='reserve' else 1)
    with self.assertRaises(r.Refus):f.reserve()

if __name__=='__main__':unittest.main()
