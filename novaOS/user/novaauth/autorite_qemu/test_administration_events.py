"""Tests hôtes de synchronisation ; aucune qualification invitée déduite."""
import re
import unittest
from administration_events import Events, Refus, command_status_pattern, observed_command_status_pattern

class Serial:
    def __init__(self, data):
        self.data=bytearray(data); self.offset=len(data); self.pending=b''
    def expect(self, pattern, seconds):
        self.data.extend(self.pending); self.pending=b''
        m=re.search(pattern,self.data[self.offset:])
        if m is None: raise TimeoutError(pattern)
        self.offset+=m.end();return m

class EventTests(unittest.TestCase):
    def test_bob_announcement_before_registry_ready(self):
        message=b'NAR2 Bob attente declenchement authentification'
        old=b'DEBUG pid=11 '+message+b'\r\n'
        s=Serial(old)
        boundary=s.offset
        s.pending=b'DEBUG pid=11 '+message+b'\r\nDEBUG pid=1 NAEXEC registre pret puis connexion du vrai nsh\r\n'
        e=Events.before_ready(s, rb'NAEXEC registre pret puis connexion du vrai nsh', 1)
        saved=s.offset
        self.assertEqual(e.start,boundary)
        self.assertEqual(e.actor(message),11)
        e.expect(11,message,1)
        self.assertEqual(s.offset,saved)
        self.assertTrue(e.records[0]['deja_recu'])
        with self.assertRaises(Refus):e.expect(11,message,1)

    def test_absent_announcement_is_not_success(self):
        s=Serial(b'');s.pending=b'NAEXEC registre pret puis connexion du vrai nsh\n'
        e=Events.before_ready(s, rb'NAEXEC registre pret puis connexion du vrai nsh', 1)
        with self.assertRaises(Refus):e.actor(b'NAR2 Bob attente declenchement authentification')

    def test_event_already_consumed_by_command_is_observed_once(self):
        s=Serial(b'DEBUG pid=11 ready\r\ncommand done\n'); pos=s.offset
        e=Events(s,0);e.expect(11,b'ready',1)
        self.assertEqual(s.offset,pos); self.assertTrue(e.records[0]['deja_recu'])
        with self.assertRaises(Refus):e.expect(11,b'ready',1)
    def test_new_event_preserves_command_cursor_and_bytes(self):
        s=Serial(b'old\n');s.pending=b'DEBUG pid=11 ready\n';pos=s.offset
        e=Events(s,pos);e.expect(11,b'ready',1)
        self.assertEqual(s.offset,pos);self.assertFalse(e.records[0]['deja_recu'])
    def test_other_pid_or_previous_scenario_does_not_match(self):
        s=Serial(b'DEBUG pid=5 ready\n');e=Events(s,0)
        with self.assertRaises(TimeoutError):e.expect(11,b'ready',1)
        e=Events(s,len(s.data))
        with self.assertRaises(TimeoutError):e.expect(5,b'ready',1)
    def test_duplicate_marker_refused(self):
        s=Serial(b'DEBUG pid=11 ready\n'*2);pos=s.offset
        with self.assertRaises(Refus):Events(s,0).expect(11,b'ready',1)
        self.assertEqual(s.offset,pos)
    def test_actor_requires_unique_event(self):
        s=Serial(b'DEBUG pid=11 auth\n');e=Events(s,0)
        self.assertEqual(e.actor(b'auth'),11)
        s.data.extend(b'DEBUG pid=5 auth\n')
        with self.assertRaises(Refus):e.actor(b'auth')


class CommandStatusTests(unittest.TestCase):
    def test_real_aarch64_interleaving_after_complete_status(self):
        # Suffixe exact de l'échec AArch64/P9-512, essai 01.
        raw = b'NXR2_1_0__FINDEBU\r\nG pid=11 NAR2 Bob fermetnsh> ure acquittee\r\n'
        s=Serial(b''); s.pending=raw
        self.assertIsNone(re.search(rb'NXR2_1_(\d+)__FIN\r?\n',raw))
        m=s.expect(command_status_pattern('NXR2_1_'),1)
        self.assertEqual(m[1],b'0')
        self.assertEqual(bytes(s.data[s.offset:]),raw[len(b'NXR2_1_0__FIN'):])
        self.assertIsNotNone(s.expect(rb'nsh> ',1))
    def test_atomic_status_survives_interleaved_prompt(self):
        # Fin exacte de l'essai S9 AArch64/P9-512, tentative 03.
        raw=(b'DEBUG pid=5 NXR2_1_0__FIN\r\n'
             b'DEBUG pid=11 NAR2 Bob attennte reash> ctivation explicite\r\n')
        self.assertIsNone(re.search(rb'nsh> ',raw))
        m=re.search(observed_command_status_pattern('NXR2_1_'),raw)
        self.assertEqual((int(m[1]),int(m[2])),(5,0))
        for incomplete in (b'',b'DEBUG pid=5 NXR2_1_0__FI',b'NXR2_1_0__FIN'):
            self.assertIsNone(re.search(observed_command_status_pattern('NXR2_1_'),incomplete))

    def test_normal_status_and_native_failure_are_preserved(self):
        for code in [0,3,12,20]:
            m=re.search(command_status_pattern('NXR2_2_'),f'NXR2_2_{code}__FIN\r\n'.encode())
            self.assertEqual(int(m[1]),code)
    def test_echo_incomplete_other_step_and_absence_do_not_pass(self):
        for raw in [b'',b'echo NXR2_1_$?__FIN',b'NXR2_1_0__FI',b'NXR2_0_0__FIN']:
            self.assertIsNone(re.search(command_status_pattern('NXR2_1_'),raw))
    def test_previous_command_not_reused(self):
        s=Serial(b'NXR2_1_0__FIN')
        with self.assertRaises(TimeoutError):s.expect(command_status_pattern('NXR2_1_'),1)
    def test_invalid_step_refused(self):
        with self.assertRaises(Refus):command_status_pattern('NXR2_.*_')

if __name__=='__main__':unittest.main()
