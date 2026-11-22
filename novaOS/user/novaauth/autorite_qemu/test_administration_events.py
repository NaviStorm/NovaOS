"""Tests hôtes de synchronisation ; aucune qualification invitée déduite."""
import re
import unittest
from administration_events import Events, Refus

class Serial:
    def __init__(self, data):
        self.data=bytearray(data); self.offset=len(data); self.pending=b''
    def expect(self, pattern, seconds):
        self.data.extend(self.pending); self.pending=b''
        m=re.search(pattern,self.data[self.offset:])
        if m is None: raise TimeoutError(pattern)
        self.offset+=m.end();return m

class EventTests(unittest.TestCase):
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

if __name__=='__main__':unittest.main()
