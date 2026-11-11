"""Qualification native, sur des images raw neuves. Les contrôles host ne soumettent aucune I/O disque."""
import argparse
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import time

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'scripts'))
from bloc_qemu import command

def qmp(vm, op, **args):
    vm.stdin.write(json.dumps(dict(execute=op,arguments=args)).encode()+b'\n')
    vm.stdin.flush()
    end=time.monotonic()+8
    while time.monotonic()<end:
        if not select.select([vm.stdout],[],[],max(0,end-time.monotonic()))[0]:break
        line=vm.stdout.readline()
        if not line:break
        # qemu-io annonce son point d'arrêt sur stdout, séparément de QMP.
        start=line.find(b'{')
        if start<0:continue
        try: item=json.loads(line[start:])
        except ValueError:continue
        if 'error' in item:raise RuntimeError(str(item['error']))
        if 'return' in item:return item['return']
    raise RuntimeError(f'Pas de réponse QMP à {op}')

class Bench:
    def __init__(self, path, arch, sector):
        self.tmp=path;self.arch=arch;self.sector=sector;self.image=path/'data.raw'
        self.env={**os.environ,'NOVA_ARCH':arch}
        self.report=ROOT/'build'/f'bloc-{arch}-{sector}'
        self.report.mkdir(parents=True,exist_ok=True)
    def run(self,case,label=None,*,debug=None,flight=False,kill=False,readonly=False,cut=None,timeout=None):
        iso=ROOT/'build'/f'bloc-{self.arch}-{case}.iso'
        if not iso.exists() or iso.stat().st_mtime<(ROOT/'target'/({'x86_64':'x86_64-unknown-none','aarch64':'aarch64-unknown-none-softfloat'}[self.arch])/'release/novaos').stat().st_mtime:
            subprocess.run(['sh',ROOT/'scripts/iso.sh',case,iso],env=self.env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,check=True)
        log=self.report/((label or case)+'.serial');log.write_bytes(b'')
        controls=['-display','none','-monitor','none','-serial','file:'+str(log)]
        monitor = flight or cut is not None
        if monitor:controls+=['-qmp','stdio']
        argv=command(self.arch,self.sector,self.image,debug=debug,readonly=readonly,controls=controls)
        with (self.report/((label or case)+'.qemu')).open('w') as err,subprocess.Popen(
                argv,env={**self.env,'NOVA_ISO':str(iso)},stdin=subprocess.PIPE if monitor else subprocess.DEVNULL,
                stdout=subprocess.PIPE if monitor else err,stderr=err,bufsize=0) as vm:
            try:
                if monitor:qmp(vm,'qmp_capabilities')
                armed=released=False;end=time.monotonic()+(timeout or (160 if case=='storage-perf' else 55))
                while time.monotonic()<end and vm.poll() is None:
                    # Conserver seulement les lignes dont le pilote série a terminé l'émission.
                    s=log.read_text(errors='replace').rpartition('\n')[0]
                    if 'Panique du noyau' in s:raise AssertionError(s[-450:])
                    if flight:
                        def control(cmd):
                            r=qmp(vm,'human-monitor-command',**{'command-line':f'qemu-io d "{cmd}"'})
                            if r:raise RuntimeError(str(r))
                        if not armed and 'DMA injection prete' in s:control('break read_aio retard');armed=True
                        if armed and not released and 'DMA quarantaine cadres epingles' in s:control('resume retard');released=True
                    if cut and cut in s:
                        if cut == "COUPURE armer blkdebug":
                            if not armed:
                                reply=qmp(vm,"human-monitor-command",**{"command-line":'qemu-io d "break flush_to_disk barriere"'})
                                if reply:raise RuntimeError(str(reply))
                                armed=True
                            if "COUPURE soumission FLUSH" not in s:
                                time.sleep(.01)
                                continue
                            reply=qmp(vm,"human-monitor-command",**{"command-line":'qemu-io d "wait_break barriere"'})
                            if reply:raise RuntimeError(str(reply))
                        stats=next(r["stats"] for r in qmp(vm,"query-blockstats") if r["device"]=="d")
                        print(f"{case} {stats['wr_operations']} ecritures {stats['flush_operations']} flush termines",flush=True)
                        break
                    if kill and ('NVFS FLUSH confirme' in s or 'NVFS adaptateur FLUSH confirme' in s):break
                    if any(w in s for w in ('BLOC session terminee','DMA bilan','DMA utilisateurs bilan')):break
                    time.sleep(.01)
                else:raise RuntimeError(f'{case}: délai expiré ou QEMU arrêté ; {log}')
                if cut:assert cut in s, s[-1600:]
                if flight:assert armed and released and 'transfert_en_vol=true' in s and 'faute=' in s
            finally:
                if vm.poll() is None:vm.kill()
                vm.wait()
        return log.read_text(errors='replace')
    def check(self,name):
        print(f'OK — {self.arch}/{self.sector} : {name}',flush=True)
    def qualify(self):
        raw=bytearray(1024*1024);raw[8192:12288]=bytes(0x71+i%83 for i in range(4096));self.image.write_bytes(raw)
        for case,label in [('dma-qual','motif invité et retrait'),('dma-fence','protections DMA et interruption pendante'),
                           ('dma-user','baux, descripteurs, panne et relance'),('dma-flight','panne en vol et quarantaine')]:
            flight=case=='dma-flight'
            s=self.run(case,debug='' if flight else None,flight=flight)
            if case=='dma-qual':assert 'DMA motif correct IRQ=1' in s and 'DMA retrait acquitte' in s
            if case=='dma-fence':
                assert s.count('erreur=Err(Fault) faute=Err(Fault) temoin=true')==2
                assert 'interruption masquee en attente retiree' in s and 'IRQ tardive acquittee sans livraison' in s
            assert ('DMA bilan Ok(())' if case in ('dma-qual','dma-fence') else 'DMA utilisateurs bilan OK') in s
            assert self.image.read_bytes()==raw
            self.check(label)
    def durable(self):
        original=bytes([0xc5])*(1<<20);self.image.write_bytes(original)
        s=self.run('storage-write',kill=True)
        assert 'NVFS WRITE accepte' in s and 'NVFS FLUSH confirme' in s
        size=self.sector;lba=8192//size
        expected=bytearray(original)
        expected[8192:8192+size]=bytes((((29*lba+37*i)&255)^((i>>5)&255)^0xb7)for i in range(size))
        assert self.image.read_bytes()==expected
        s=self.run('storage-read')
        assert 'NVFS READ octets compares' in s and 'BLOC session terminee ok=true' in s
        assert self.image.read_bytes()==expected
        self.check('WRITE + FLUSH, arrêt, nouvelle VM, READ et image entière')
    def adapter(self):
        before = bytes([0x53]) * (1 << 20)
        self.image.write_bytes(before)
        page = bytes(((2 * 29 + i * 37) & 255) ^ ((i >> 5) & 255) ^ 0xb7 for i in range(4096))
        expected = before[:8192] + page + before[12288:]
        write = self.run('storage-page-write', kill=True)
        assert 'NVFS coeur initialise ELF tas prive' in write
        assert 'NVFS adaptateur FLUSH confirme' in write and self.image.read_bytes() == expected
        read = self.run('storage-page-read')
        assert 'NVFS coeur initialise ELF tas prive' in read
        assert 'NVFS adaptateur READ 4096 confirme' in read and 'BLOC session terminee ok=true' in read
        assert self.image.read_bytes() == expected
        self.check('Device NovaFS, page complète de 4 Kio entre deux VM')
        denied = self.run('storage-page-write', 'page-lecture-seule', readonly=True)
        assert 'service=Exit(5)' in denied and 'client=Exit(5)' in denied
        assert 'NVFS adaptateur FLUSH confirme' not in denied
        assert self.image.read_bytes() == expected
        self.check('Device NovaFS, refus du support RO transmis au client')
    def failures(self):
        original=bytes([0xc5])*(1<<20)
        for label,event,case in [('eio-read','read_aio','storage-read'),('eio-write','write_aio','storage-write'),('eio-flush','flush_to_disk','storage-write')]:
            baseline=bytearray(original)
            if label=='eio-read':
                lba=8192//self.sector
                baseline[8192:8192+self.sector]=bytes((((29*lba+37*i)&255)^((i>>5)&255)^0xb7)for i in range(self.sector))
            self.image.write_bytes(baseline);cfg=self.tmp/'inject.conf'
            prefix='';attrs='sector = "16"\n'
            if label=='eio-flush':
                prefix='[set-state]\nevent = "write_aio"\nstate = "1"\nnew_state = "2"\n';attrs='state = "2"\n'
            cfg.write_text(prefix+f'[inject-error]\nevent = "{event}"\nerrno = "5"\n'+attrs)
            s=self.run(case,label,debug=cfg)
            assert 'BLOC session terminee ok=false' in s and 'NVFS FLUSH confirme' not in s and 'retrait=Ok(())' in s,(label,s[-500:])
            kind={'eio-read':0,'eio-write':1,'eio-flush':2}[label]
            assert f'BLOC erreur peripherique op={kind} statut=1' in s
            if label=='eio-flush':
                assert 'NVFS WRITE accepte' in s
                after=self.image.read_bytes()
                assert after[:8192]==original[:8192] and after[8192+self.sector:]==original[8192+self.sector:]
            else:assert self.image.read_bytes()==baseline
            self.check(label)
        self.image.write_bytes(original)
        s=self.run('storage-write','lecture-seule',readonly=True)
        assert 'BLOC session terminee ok=false' in s and 'NVFS WRITE accepte' not in s and self.image.read_bytes()==original
        self.check('droit en écriture refusé')
        s=self.run('storage-live','support-manuel',readonly=True)
        assert 'NVFS INFO support accessible' in s and 'BLOC session terminee ok=true' in s and self.image.read_bytes()==original
        self.check('profil de démarrage manuel en lecture seule')
    def perf(self):
        self.image.write_bytes(bytes(1<<20));s=self.run('storage-perf')
        assert 'BLOC session terminee ok=true' in s and s.count('BLOC parcours ')==4 and s.count('BLOC mesure ')==6
        self.check('mesures natives, 128 transferts vérifiés')
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--perf',action='store_true')
    parser.add_argument('--only',choices=('qualify','durable','failures','adapter'))
    args=parser.parse_args()
    for size in (512,4096):
        with tempfile.TemporaryDirectory(prefix='novabloc-')as tmp:
            bench=Bench(Path(tmp),os.environ.get('NOVA_ARCH','x86_64'),size)
            if args.perf:bench.perf()
            elif args.only:getattr(bench,args.only)()
            else:bench.qualify();bench.durable();bench.failures();bench.adapter()
