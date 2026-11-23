"""Préconditions du raccord et préimages historiques, sans exécution QEMU."""
import ast
from pathlib import Path
import unittest
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
if __name__=='__main__':unittest.main()
