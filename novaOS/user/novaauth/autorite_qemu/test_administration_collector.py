"""Sélection du vCPU observé ; oracles hôtes, pas preuves invitées."""
import unittest
from administration_collector_qemu import stopped_thread, advance_probe, probe_kind
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


class BoundedStep(unittest.TestCase):
    def run_step(self, pcs, stacks=None, cpu='01'):
        class Debug:
            def __init__(self): self.i=-1;self.steps=0
            def register(self, index):
                if index==31:return 80 if stacks is None or self.i<0 else stacks[self.i]
                return pcs[self.i]
            def command(self, packet):
                if packet.startswith('vCont;s:'):
                    self.i+=1;self.steps+=1;return 'T05thread:'+cpu+';'
                return 'QC'+cpu if packet=='qC' else 'OK'
        d=Debug();events=[]
        try:advance_probe(d,'aarch64',100,'01',lambda **kw:events.append(kw))
        finally:self.steps=d.steps;self.events=events
    def test_one_stalled_stop_then_real_progress(self):
        self.run_step([100,104]);self.assertEqual(self.steps,2)
        self.assertEqual([r['pc'] for r in self.events],[100,104])
    def test_stall_is_bounded_and_never_success(self):
        with self.assertRaises(Refus):self.run_step([100,100,100])
        self.assertEqual(self.steps,3)
    def test_wrong_cpu_and_changed_stack_are_not_retryable(self):
        with self.assertRaises(Refus):self.run_step([100],cpu='02')
        self.assertEqual(self.steps,1)
        with self.assertRaises(Refus):self.run_step([100],stacks=[64])
        self.assertEqual(self.steps,1)


class ProbeAliasing(unittest.TestCase):
    def test_exact_aarch64_alias_preserves_session_then_output(self):
        same=0xffffffff8009e990
        probes={'session':same,'output':same}
        self.assertEqual([probe_kind(probes,same,n) for n in (80,272)],['session','output'])

    def test_distinct_symbols_keep_their_formats(self):
        probes={'session':100,'output':200}
        self.assertEqual(probe_kind(probes,100,80),'session')
        self.assertEqual(probe_kind(probes,200,272),'output')
        for pc,n in ((100,272),(200,80),(300,80),(100,79),(200,273)):
            with self.assertRaises(Refus):probe_kind(probes,pc,n)


if __name__=='__main__': unittest.main()
