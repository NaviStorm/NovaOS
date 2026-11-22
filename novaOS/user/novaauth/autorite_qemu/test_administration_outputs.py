"""Oracles hôtes du collecteur ; ne constituent pas une qualification QEMU."""
import unittest
from administration_outputs import Collector, Refus, HEADER, read_stopped

A = (10, b'A'*32, b'a'*32)
B = (20, b'B'*32, b'b'*32)


def record(session=A, child=30, fd=1, endpoint=100, event=0, seq=0, data=b'', **changes):
    values = dict(magic=b'NXR2OUT1', version=1, size=272, event=event, fd=fd,
                  length=len(data), shell=session[0], child=child, instance=session[1],
                  child_instance=bytes([child])*32, sid=session[2], endpoint=endpoint,
                  seq=seq, payload=data.ljust(128, b'\0'))
    values.update(changes)
    return HEADER.pack(*values.values())


class Outputs(unittest.TestCase):
    def test_both_sessions_stdout_stderr_exact_without_mixing(self):
        c = Collector([A, B])
        for session, child, base in [(A, 30, 100), (B, 40, 200)]:
            for fd in (1, 2):
                kw = dict(session=session, child=child, endpoint=base+fd, fd=fd)
                c.accept(record(**kw))
                for seq, data in enumerate([b'\x00\t\xff', bytes(range(128)), b'\r\n\x1b[A'], 1):
                    c.accept(record(**kw, event=1, seq=seq, data=data))
                c.accept(record(**kw, event=2, seq=4))
                self.assertEqual(c.output(*session, child, fd), b'\x00\t\xff'+bytes(range(128))+b'\r\n\x1b[A')
        self.assertTrue(all(s.closed for s in c.streams.values()))
        with self.assertRaises(Refus): c.output(*B, 30, 1)
        with self.assertRaises(Refus): c.output(*A, 40, 1)

    def test_substitution_every_binding_field_and_stream_is_rejected(self):
        for changes in [dict(shell=20), dict(instance=B[1]), dict(sid=B[2]),
                        dict(child=31), dict(child_instance=b'C'*32), dict(endpoint=101), dict(fd=2)]:
            with self.subTest(changes=changes):
                c=Collector([A,B]); c.accept(record())
                with self.assertRaises(Refus): c.accept(record(event=1,seq=1,data=b'X',**changes))
                self.assertEqual(c.output(*A,30,1), b'')

    def test_duplicate_skipped_or_closed_sequences(self):
        for seq in (0,2,2**64-1):
            c=Collector([A]); c.accept(record())
            with self.assertRaises(Refus): c.accept(record(event=1,seq=seq,data=b'X'))
        c=Collector([A]); c.accept(record()); c.accept(record(event=1,seq=1,data=b'X'))
        with self.assertRaises(Refus): c.accept(record(event=1,seq=1,data=b'X'))
        c.accept(record(event=2,seq=2))
        with self.assertRaises(Refus): c.accept(record(event=1,seq=3,data=b'Y'))
        with self.assertRaises(Refus): c.accept(record())

    def test_malformed_noncanonical_or_unknown_frame(self):
        variants=[record()[:-1],record()+b'\0']
        for changes in [dict(version=2),dict(size=271),dict(event=3),dict(fd=0),dict(length=129),
                        dict(payload=b'X'+b'\0'*127),dict(child=10),dict(endpoint=0),dict(child_instance=b'\0'*32)]:
            variants.append(record(**changes))
        variants.extend([record(event=1,seq=1),record(event=2,seq=1,data=b'X')])
        for raw in variants:
            with self.subTest(raw=raw[:16]):
                with self.assertRaises(Refus): Collector([A]).accept(raw)

    def test_saturation_is_explicit_without_partial_data(self):
        c=Collector([A],byte_limit=3); c.accept(record())
        c.accept(record(event=1,seq=1,data=b'ABC'))
        with self.assertRaises(Refus): c.accept(record(event=1,seq=2,data=b'D'))
        self.assertEqual(c.output(*A,30,1),b'ABC')
        self.assertEqual(c.streams[100].sequence,1)
        c=Collector([A])
        for i in range(8): c.accept(record(child=30+i,endpoint=100+i))
        with self.assertRaises(Refus): c.accept(record(child=38,endpoint=108))
        self.assertEqual(len(c.streams),8)

    def test_closed_bindings_do_not_allow_unbounded_history(self):
        c=Collector([A])
        for i in range(256):
            c.accept(record(endpoint=100+i))
            c.accept(record(endpoint=100+i,event=2,seq=1))
        with self.assertRaises(Refus): c.accept(record(endpoint=356))
        self.assertEqual(len(c.streams),256)
        self.assertEqual(c.bytes,0)

    def test_sessions_cannot_share_identity_or_session(self):
        for sessions in [[A,A],[A,(20,A[1],B[2])],[A,(20,B[1],A[2])],[A,(10,B[1],B[2])]]:
            with self.assertRaises(Refus): Collector(sessions)

    def test_same_child_cannot_cross_sessions_or_duplicate_fd(self):
        c=Collector([A,B]); c.accept(record())
        with self.assertRaises(Refus): c.accept(record(session=B,endpoint=101))
        with self.assertRaises(Refus): c.accept(record(endpoint=101))

    def test_gdb_collector_only_reads_memory_on_both_architectures(self):
        class Debug:
            def __init__(self,arch): self.arch=arch;self.calls=[]
            def register(self,index):
                self.calls.append(('p',index))
                return 0x1234 if index==(0 if self.arch=='aarch64' else 5) else 272
            def command(self,command): self.calls.append(command);return record().hex()
        for arch in ['aarch64','x86_64']:
            d=Debug(arch); self.assertEqual(read_stopped(d),record())
            self.assertEqual(d.calls[-1],'m1234,110')
            self.assertEqual(len(d.calls),3)


if __name__=='__main__': unittest.main()
