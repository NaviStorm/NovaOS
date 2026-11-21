import unittest
from auth_records import decisions
class AuthRecords(unittest.TestCase):
    def test_fixed_record_and_expiration(self):
        values=[7,100,1600,104,1604,5,0,1608]
        line=b'NXAUTH '+b'ab'*32+b' '+b''.join(f'{v:016x} '.encode() for v in values)+b'\r\n'
        self.assertEqual(len(line)-2,208)
        d=decisions(line)[0]
        self.assertEqual(d['retour'],5);self.assertEqual(d['verification']-d['emission'],1504)
        self.assertEqual(d['defi'],'ab'*32)
        for raw in (line[:-2],line.replace(b'ab',b'zz',1),line.replace(b'0000000000000007 ',b'7 ',1)):
            self.assertEqual(decisions(raw),[])
    def test_historical_trace_is_still_readable(self):
        raw=b'NXLOGIN refus defi expire\nNXBAIL op=NSHOPEN1 tag=2 decision=0 valide=0 echeance=0 retour=5 fin=17000\r\n'
        d=decisions(raw)[0];self.assertEqual(d['retour'],5);self.assertTrue(d['expire'])
    def test_fresh_challenge_is_not_inferred_from_old(self):
        def line(d):return b'NXAUTH '+d*32+b' '+b'0000000000000000 '*8+b'\n'
        ds=decisions(line(b'ab')+line(b'cd'));self.assertNotEqual(ds[0]['defi'],ds[1]['defi'])
if __name__=='__main__':unittest.main()
