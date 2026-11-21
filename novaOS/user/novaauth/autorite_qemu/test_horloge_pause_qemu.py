import unittest,ssl
from unittest.mock import patch
from horloge_pause_qemu import Clock,Stream
class Pause(unittest.TestCase):
    def test_temps_actif_et_suspension_explicitement_bornee_par_le_banc(self):
        now=[0.0];c=Clock(lambda:now[0])
        now[0]=10;c.pause();c.pause()
        now[0]=90;self.assertEqual(c.active(),10);self.assertEqual(c.excluded(),80)
        c.resume();c.resume();now[0]=100
        self.assertEqual(c.active(),20);self.assertEqual(c.excluded(),80)
        c.pause();now[0]=110;c.resume();now[0]=120
        self.assertEqual(c.active(),30);self.assertEqual(c.excluded(),90)
    def test_envois_partiels_sans_repetition_des_octets(self):
        class Socket:
            def __init__(self):self.data=bytearray()
            def setblocking(self,value):assert not value
            def send(self,data):
                n=min(3,len(data));self.data.extend(data[:n]);return n
        s=Socket();Stream(s,Clock()).sendall(b'0123456789')
        self.assertEqual(s.data,b'0123456789')
    def test_lecture_dans_tampon_apres_attente_tls(self):
        class Socket:
            def __init__(self):self.calls=0
            def setblocking(self,value):assert not value
            def recv_into(self,buffer):
                self.calls+=1
                if self.calls==1:raise ssl.SSLWantReadError()
                buffer[:3]=b'abc';return 3
        s=Socket();buffer=bytearray(8)
        with patch('horloge_pause_qemu.select.select'):
            self.assertEqual(Stream(s,Clock()).recv_into(buffer),3)
        self.assertEqual(buffer,b'abc'+bytes(5));self.assertEqual(s.calls,2)
    def test_delai_actif_reste_borne(self):
        now=[0.0]
        class Socket:
            def setblocking(self,value):pass
            def recv(self,n):raise ssl.SSLWantReadError()
        stream=Stream(Socket(),Clock(lambda:now[0]),limit=1)
        def advance(*args):now[0]+=0.4
        with patch('horloge_pause_qemu.select.select',advance):
            with self.assertRaises(TimeoutError):stream.recv(1)
        self.assertLessEqual(now[0],1.3)
if __name__=='__main__':unittest.main()
