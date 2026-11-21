import unittest
from exec_resources import verify, memory_cycles
from reprise import Refus

def sample(phase,ipc=58,free=1000):
    data=(f'NAEXEC ressources phase={phase} owner=5 roots=2 handles=8 endpoints=49 nodes=102 waits=12 processus=15 threads=1 joins=0 cadres_ipc={ipc} cadres_libres=Ok({free})\r\n').encode()
    for pid,service,roots,handles in [(5,0,2,8),(7,22,4,7),(8,23,4,6)]:
        data+=(f'NAEXEC acteur phase={phase} owner=5 pid={pid} service={service} roots={roots} handles={handles} waits=0 pages=66 pile=64 threads=1 cadres_ipc={ipc}\r\n').encode()
    return data

class Resources(unittest.TestCase):
    def test_actor_measurements_are_not_additional_spawn_calls(self):
        raw=sample('consommation')+sample('refus_sans_creation')
        self.assertEqual(raw.count(b'NAEXEC ressources phase=consommation '),1)
        self.assertEqual(raw.count(b'phase=consommation'),4)
    def test_plateau_after_measured_warmup_and_additional_cycles(self):
        raw=b''
        for i in range(6):
            free=1020 if i==0 else 988
            raw+=sample('avant_ticket',58,free-20)+sample('consommation',59,free-21)+sample('fermeture',59,free)
            values=[100,2,4,0,1,100,2,4,0,2,40,40,1065472]
            raw+=b'NXMEM '+b''.join(f'{v:016x} '.encode() for v in values)+b'\r\n'
            if i==2:self.assertTrue(verify(raw,warm=3)['plateau_chaud'])
        self.assertTrue(verify(raw,warm=6)['plateau_chaud'])
        with self.assertRaises(Refus):verify(raw.replace(b'cadres_libres=Ok(988)',b'cadres_libres=Ok(900)'),warm=6)
        # Une croissance tardive n'est pas cachée par le plateau initial.
        changed=raw.rsplit(b'cadres_libres=Ok(988)',1)
        with self.assertRaises(Refus):verify(b'cadres_libres=Ok(987)'.join(changed),warm=6)
    def test_physical_growth_or_unexplained_live_growth_is_rejected(self):
        valid=[100,2,4,0,1,100,2,4,0,2,40,40,1065472]
        def line(v):return b'NXMEM '+b''.join(f'{x:016x} '.encode() for x in v)+b'\n'
        raw=line(valid)*3
        self.assertTrue(memory_cycles(raw,3)[2]['reutilisation_sans_attribution_physique'])
        for index,value in [(7,5),(5,101),(12,1065473)]:
            bad=valid.copy();bad[index]=value
            with self.assertRaises(Refus):memory_cycles(line(valid)*2+line(bad),3)
    def test_missing_actor_or_endpoint_leak_is_rejected(self):
        raw=sample('avant_ticket')+sample('consommation')+sample('fermeture')
        self.assertTrue(verify(raw)['restitution_verifiee'])
        with self.assertRaises(Refus):verify(raw.replace(b'phase=fermeture owner=5 pid=8',b'phase=absent owner=5 pid=8'))
        with self.assertRaises(Refus):verify(raw.replace(b'phase=fermeture owner=5 roots=2 handles=8 endpoints=49',b'phase=fermeture owner=5 roots=2 handles=8 endpoints=50'))
    def test_refused_probe_cannot_grow_pool(self):
        raw=sample('consommation')+sample('refus_sans_creation')
        self.assertTrue(verify(raw,probe=True)['restitution_verifiee'])
        with self.assertRaises(Refus):verify(sample('consommation')+sample('refus_sans_creation',59),probe=True)
    def test_actor_threads_and_pages_are_not_global_noise(self):
        raw=sample('avant_ticket')+sample('consommation')+sample('fermeture')
        for old,new in [(b'phase=fermeture owner=5 pid=7 service=22 roots=4 handles=7',b'phase=fermeture owner=5 pid=7 service=22 roots=4 handles=8'),(b'phase=fermeture owner=5 pid=8 service=23 roots=4 handles=6 waits=0 pages=66',b'phase=fermeture owner=5 pid=8 service=23 roots=4 handles=6 waits=0 pages=67')]:
            with self.assertRaises(Refus):verify(raw.replace(old,new))
if __name__=='__main__':unittest.main()
