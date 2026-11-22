"""Sélection du vCPU observé ; oracles hôtes, pas preuves invitées."""
import unittest
from administration_collector_qemu import stopped_thread
from administration_outputs import Refus


class StoppedCPU(unittest.TestCase):
    def test_selects_stop_packet_thread_not_previous_current_cpu(self):
        class Debug:
            def __init__(self): self.calls=[]
            def command(self, packet): self.calls.append(packet); return 'OK'
        d=Debug()
        self.assertEqual(stopped_thread(d,'T05thread:02;'),'02')
        self.assertEqual(d.calls,['Hg02'])

    def test_refuses_missing_ambiguous_or_invalid_thread(self):
        class Debug:
            def command(self, packet): raise AssertionError(packet)
        for stop in ('S05','T05thread:00;','T05thread:01;thread:02;','T05thread:-1;'):
            with self.assertRaises(Refus): stopped_thread(Debug(),stop)

    def test_refuses_unsuccessful_selection(self):
        class Debug:
            def command(self, packet): return 'E01'
        with self.assertRaises(Refus): stopped_thread(Debug(),'T05thread:01;')


if __name__=='__main__': unittest.main()
