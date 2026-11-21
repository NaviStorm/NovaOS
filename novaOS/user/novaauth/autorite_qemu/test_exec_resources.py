import unittest
from exec_resources import verify, memory_cycles, retired_family, final_memory_cycles
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
            values=[100,2,4,0,1,100,2,4,0,2,40,40,1065472,1,32,0,2,64,0]
            raw+=b'NXMEM4 '+b''.join(f'{v:012x} '.encode() for v in values)+b'\r\n'
            raw+=b'TMP retrait confirme NXFIN5 '+b''.join(f'{v:012x} '.encode() for v in [100,2,4,0,2,2,64,0,40,1065472,24,8])+b'\r\n'
            if i==2:self.assertTrue(verify(raw,warm=3)['plateau_chaud'])
        self.assertTrue(verify(raw,warm=6)['plateau_chaud'])
        with self.assertRaises(Refus):verify(raw.replace(b'cadres_libres=Ok(988)',b'cadres_libres=Ok(900)'),warm=6)
        # Une croissance tardive n'est pas cachée par le plateau initial.
        changed=raw.rsplit(b'cadres_libres=Ok(988)',1)
        with self.assertRaises(Refus):verify(b'cadres_libres=Ok(987)'.join(changed),warm=6)
    def test_physical_growth_or_unexplained_live_growth_is_rejected(self):
        valid=[100,2,4,0,1,100,2,4,0,2,40,40,1065472,1,32,0,2,64,0]
        def line(v):return b'NXMEM4 '+b''.join(f'{x:012x} '.encode() for x in v)+b'\n'
        raw=line(valid)*3
        self.assertTrue(memory_cycles(raw,3)[2]['reutilisation_sans_attribution_physique'])
        for index,value in [(7,5),(5,101),(12,1065473),(16,1),(17,32),(18,1)]:
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
    def test_forced_family_retirement_requires_real_kernel_restitution(self):
        raw=b'NAEXEC resident phase=consommation pid=7 service=22 pages=1 pile=64\nNAEXEC resident phase=consommation pid=8 service=23 pages=1 pile=64\nNAEXEC retrait_famille manager=2 pid=7 service=22 statut=125\nNAEXEC retrait_famille manager=2 pid=8 service=23 statut=125\n'
        for pid in (7,8):
            raw+=f'NAEXEC restitue pid={pid} roots=0 handles=0 waits=0 pages=0 pile=0 threads=0 endpoints=0 nodes=0 espaces=0 cadres_ipc=0 cadres_libres=Ok(1000)\n'.encode()
        self.assertTrue(retired_family(raw)['drainage_verifie'])
        persistent=b'NAEXEC resident phase=consommation pid=9 service=0 pages=100 pile=64\n'+raw.replace(b'endpoints=0 nodes=0 espaces=0',b'endpoints=2 nodes=4 espaces=1')
        persistent+=b'NAEXEC survivant apres_pid=8 pid=9 service=0 roots=2 handles=4 waits=0 pages=100 pile=64\n'
        self.assertTrue(retired_family(persistent)['drainage_verifie'])
        for wrong in (persistent.replace(b'nodes=4',b'nodes=5'),persistent.replace(b'apres_pid=8 pid=9',b'apres_pid=8 pid=10')):
            with self.assertRaises(Refus):retired_family(wrong)

        for wrong in (raw.replace(b'handles=0',b'handles=1'),raw.replace(b'pid=8 roots=',b'pid=9 roots='),raw.replace(b'espaces=0',b'espaces=1'),raw.replace(b'statut=125',b'statut=0')):
            with self.assertRaises(Refus):retired_family(wrong)
    def test_final_memory_must_plateau_after_tmp_cleanup(self):
        def line(v):return b'TMP retrait confirme NXFIN5 '+b''.join(f'{x:012x} '.encode() for x in v)+b'\n'
        valid=[100,2,4,0,2,2,64,0,40,1065472,24,8]
        self.assertEqual(len(final_memory_cycles(line(valid)*6,6)),6)
        for index in (0,1,2,3,7,8,9,10,11):
            changed=valid.copy();changed[index]+=1
            with self.assertRaises(Refus):final_memory_cycles(line(valid)*5+line(changed),6)
    def test_later_decrease_requires_exact_space_attribution_without_growth(self):
        def line(v):return b'TMP retrait confirme NXFIN5 '+b''.join(f'{x:012x} '.encode() for x in v)+b'\n'
        base=[100,2,4,0,2,2,64,0,40,1065472,48,8]
        lower=base.copy();lower[0]-=24;lower[10]-=24
        self.assertEqual(len(final_memory_cycles(line(base)*3+line(lower)+line(base)*2,6)),6)
        # Diminution inexpliquée, croissance attribuée, et plateau 2/3 rompu refusés.
        unexplained=base.copy();unexplained[0]-=24
        growth=base.copy();growth[0]+=24;growth[10]+=24
        for bad in (unexplained,growth):
            with self.assertRaises(Refus):final_memory_cycles(line(base)*5+line(bad),6)
        with self.assertRaises(Refus):final_memory_cycles(line(base)*2+line(lower)+line(base)*3,6)
if __name__=='__main__':unittest.main()
