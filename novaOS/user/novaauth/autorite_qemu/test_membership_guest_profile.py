"""Préconditions du raccord et préimages historiques, sans exécution QEMU."""
import ast
import re
from administration_events import membership_revision_pattern, removal_race_results
from pathlib import Path
import unittest
from types import SimpleNamespace
ROOT=Path(__file__).resolve().parents[4]
class GuestProfile(unittest.TestCase):
 def test_historical_config_bound_unchanged(self):
  self.assertIn('assert!(word <= 19)',(ROOT/'novaOS/user/storage/runtime.rs').read_text())
  s=(ROOT/'novaOS/user/novaauth/service/src/bin/common/recovery_store.rs').read_text()
  self.assertIn('crate::membership_transaction::identity(23)',s)
  self.assertNotIn('rt::config(23+i)',s)
 def test_profile_is_explicit_and_rejects_local_authority(self):
  s=(ROOT/'novaOS/user/novaauth/service/profile.py').read_text()
  self.assertIn('if membership_r2 and (policies or lecture)',s)
  self.assertIn('membership_r2=False',s)
 def test_bench_parses_and_retains_negative_oracles(self):
  p=Path(__file__).with_name('administration_commandes_qemu.py');s=p.read_text();ast.parse(s)
  self.assertIn("a.membership_r2=a.lot=='retrait-selectif-vide'",s)
  self.assertIn("[(54,1),(55,1),(56,3)]",s)
  self.assertIn("ne qualifie pas Q1-Q4",s)
  self.assertIn("a.acquittement_selectif_perdu and not discarded_receipt",s)
 def test_io_reuses_bounded_private_port_not_historical_config(self):
  s=(ROOT/'novaOS/user/novaauth/service/membership_io_profile.py').read_text()
  self.assertNotIn('rt::config(21)',s)
  self.assertIn('administrative::port()?',s)
  self.assertNotIn('IPC_CREATE',s)
 def test_first_vfs_admission_initializes_fixture_epochs_like_exec(self):
  s=(ROOT/'novaOS/user/novaauth/autorite_qemu/administration_commandes_qemu.py').read_text()
  self.assertIn('(operation == 40 or (a.membership_io and operation == 57)) and not enrolled',s)
  self.assertIn("proof = admin.call('r2-enroll', state)",s)
 def test_fragmented_revision_waits_for_complete_number(self):
  pattern=membership_revision_pattern('avant','G',0)
  line=b'NARM Q1 phase=avant origine=G native=0 octets=12 revision=193\r\n'
  for end in range(len(line)):
   self.assertIsNone(re.search(pattern,line[:end]))
  self.assertEqual(int(re.search(pattern,line)[1]),193)
  self.assertIsNone(re.search(pattern,line.replace(b'origine=G',b'origine=H')))
 def test_inflight_markers_are_complete_and_never_confused_with_previous_phase(self):
  for phase in ('en-vol','apres-en-vol'):
   pattern=membership_revision_pattern(phase,'G',0 if phase=='en-vol' else 10)
   code=0 if phase=='en-vol' else 10
   line=f'NARM Q1 phase={phase} origine=G native={code} octets={12 if code==0 else 0} revision=193\r\n'.encode()
   self.assertIsNone(re.search(pattern,line[:-2]))
   self.assertEqual(int(re.search(pattern,line)[1]),193)
   self.assertIsNone(re.search(pattern,line.replace(phase.encode(),b'avant')))
 def test_removal_race_requires_one_canonical_terminal_receipt(self):
  receipt=bytes(8)+b'O'*16+(31).to_bytes(8,'little')+bytes([1])+bytes(31)
  win=bytes(16)+receipt+b'O'*16;lose=bytes(8)+(5).to_bytes(8,'little')+(5).to_bytes(8,'little')+bytes(56)+b'O'*16
  for data in (win+lose,lose+win):self.assertEqual(sum(x['confirmation']==0 for x in removal_race_results(data,31)),1)
  for data in (win+win,lose+lose,(win+lose)[:-1],win+lose+bytes(1)):
   with self.assertRaises(ValueError):removal_race_results(data,31)
  for at in (16,24,40,48,49,79,80,176):
   bad=bytearray(win+lose);bad[at]^=1
   with self.assertRaises(ValueError):removal_race_results(bytes(bad),31)
  for native in (3,7,10,12,22,24):
   bad=bytearray(win+lose);bad[104:112]=native.to_bytes(8,'little')
   with self.assertRaises(ValueError):removal_race_results(bytes(bad),31)
 def test_restarted_vfs_probe_precedes_shell_exit(self):
  source=Path(__file__).with_name('administration_commandes_qemu.py').read_text()
  node=next(n.value for n in ast.walk(ast.parse(source)) if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='line' for t in n.targets))
  expression=compile(ast.Expression(node),'<commande réelle>','eval')
  a=SimpleNamespace(reactivation=False,separation=False,membership_io=False,membership_r2=True)
  values=dict(command='whoami',step='NXR2_1_',i=1,cases=[0,1],a=a,restarted=True)
  self.assertEqual(eval(expression,{},values),'whoami; echo NXR2_1_$?__FIN')
  values['restarted']=False
  self.assertTrue(eval(expression,{},values).endswith('; exit'))
if __name__=='__main__':unittest.main()

class CancellationMarkers(unittest.TestCase):
 def test_cancelled_is_only_valid_for_the_explicit_before_go_case(self):
  import re
  marker=b'NARM Q1 phase=avant-annule origine=G native=9 octets=0 revision=0\r\n'
  self.assertIsNotNone(re.search(membership_revision_pattern('avant-annule','G',9),marker))
  for phase,origin,code in [('avant','G',9),('avant-annule','H',9),('apres-annule','G',0)]:
   with self.assertRaises(Exception):membership_revision_pattern(phase,origin,code)
