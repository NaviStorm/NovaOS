"""Oracles du banc : la syntaxe imbriquée ne change pas la demande native."""
import shlex
import unittest
from administration_commandes_qemu import command_mode

class Modes(unittest.TestCase):
    def test_exact_source(self):
        for command in ("whoami", "groups --group base", "useradd --factor-reference abc --group base carol --prepare", "echo '' 'a b' '-x'", "usermod --outcome 01"):
            self.assertEqual(shlex.split(command_mode(command, 'nsh-c')), ['nsh', '-c', command])
            self.assertEqual(command_mode(command, 'interactif'), command)
    def test_scripts_are_fixed_files(self):
        self.assertEqual(command_mode('whoami', 'script'), 'nsh /travail/r2-whoami.nsh')
        with self.assertRaises(ValueError): command_mode('whoami; false', 'script')
    def test_no_unknown_mode(self):
        with self.assertRaises(ValueError): command_mode('whoami', 'inconnu')

if __name__ == '__main__': unittest.main()
