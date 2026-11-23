"""Oracles du banc : la syntaxe imbriquée ne change pas la demande native."""
import shlex
import unittest
from administration_commandes_qemu import command_mode, initial_fragments
from reprise import Refus
from pathlib import Path
from tempfile import TemporaryDirectory

class Modes(unittest.TestCase):
    def test_exact_source(self):
        for command in ("whoami", "groups --group base", "useradd --factor-reference abc --group base carol --prepare", "echo '' 'a b' '-x'", "usermod --outcome 01"):
            self.assertEqual(shlex.split(command_mode(command, 'nsh-c')), ['nsh', '-c', command])
            self.assertEqual(command_mode(command, 'interactif'), command)
    def test_scripts_are_fixed_files(self):
        self.assertEqual(command_mode('whoami', 'script'), 'nsh /travail/r2-whoami.nsh')
        with self.assertRaises(ValueError): command_mode('whoami; false', 'script')
    def test_confirmation_values_are_data(self):
        text=command_mode('useradd --factor-reference d3e1962bb9c6e8962e338b1972b5359f --group base carol --confirm 1234 --request 5678','script')
        self.assertEqual(text,'export R2_CONFIRM=1234 R2_REQUEST=5678; nsh /travail/r2-creation-confirmer.nsh')
    def test_script_never_substitutes_business_arguments(self):
        with self.assertRaises(ValueError):command_mode('usermod --disable bob --prepare','script')
        with self.assertRaises(ValueError):command_mode('groupadd intrus --prepare','script')
    def test_thirteen_fragments_keep_numeric_order(self):
        with TemporaryDirectory() as d:
            root=Path(d);manifest=bytearray(418);manifest[378:382]=(13).to_bytes(4,'big')
            expected=[bytes([i])*17 for i in range(13)]
            for i,data in enumerate(expected):(root/f'fragment-{i}.chiffre').write_bytes(data)
            self.assertEqual(initial_fragments(root,manifest),expected)
            (root/'fragment-2.chiffre').unlink()
            with self.assertRaises(Refus):initial_fragments(root,manifest)
    def test_fragment_count_bounded_before_allocation(self):
        manifest=bytearray(418);manifest[378:382]=b'\xff'*4
        with self.assertRaises(Refus):initial_fragments(Path('.'),manifest)
        with self.assertRaises(Refus):initial_fragments(Path('.'),b'')
    def test_no_unknown_mode(self):
        with self.assertRaises(ValueError): command_mode('whoami', 'inconnu')

if __name__ == '__main__': unittest.main()
