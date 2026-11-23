"""Oracles du banc Q4, sans assimilation à une exécution invitée."""
import unittest
from reprise import Refus
from membership_cold_qemu import reservation_snapshot,verify_cold_refusal
import test_membership_retirement as fixture

class ColdOracle(unittest.TestCase):
 def setUp(self):
  self.f=fixture.Membership();self.f.setUp();self.addCleanup(self.f.doCleanups)
 def test_only_reserved_without_ack_or_unfinished_operation_is_accepted(self):
  self.f.reserve();snap=reservation_snapshot(self.f.f.path,self.f.op)
  self.assertEqual(snap['etat'],'RESERVEE');self.assertEqual(snap['mutations'],0)
  self.f.admit()
  with self.assertRaises(Refus):reservation_snapshot(self.f.f.path,self.f.op)
 def test_published_or_acknowledged_retirement_is_not_this_cut(self):
  self.f.reserve();self.f.publish()
  with self.assertRaises(Refus):reservation_snapshot(self.f.f.path,self.f.op)
 def test_refusal_must_be_the_actual_drainage_guard(self):
  reason='1 14 drainage EXEC non confirmé : nouvelle incarnation inactive\n'
  good='1 14 504 0 5 100 200\n'
  verify_cold_refusal(b'NARV registre quarantaine generation non activable',good,reason,5)
  for rpc,error,native in [(good,'défi expiré',5),(good,reason,22),(good.replace(' 5 ',' 22 '),reason,5),(good+good,reason,5),('',reason,5)]:
   with self.assertRaises(Refus):verify_cold_refusal(b'NARV registre quarantaine generation non activable',rpc,error,native)
 def test_activation_and_decryption_cannot_be_called_safe_refusal(self):
  with self.assertRaises(Refus):verify_cold_refusal(b'','1 14 504 0 5 100 200\n','drainage EXEC non confirmé : nouvelle incarnation inactive',5)
  for marker in [b'NAR2 vue registre isole',b'NAR2 triplet dechiffre',b'NAEXEC registre pret puis connexion du vrai nsh',b'NARM retrait selectif RETIREE']:
   with self.assertRaises(Refus):verify_cold_refusal(b'NARV registre quarantaine generation non activable'+marker,'1 14 504 0 5 100 200\n','drainage EXEC non confirmé : nouvelle incarnation inactive',5)

if __name__=='__main__':unittest.main()
