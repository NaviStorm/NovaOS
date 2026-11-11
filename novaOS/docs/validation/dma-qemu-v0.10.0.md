# Qualification matérielle de virtio-IOMMU et VirtIO Block

Ces deux commandes reproduisent les mesures du [rapport de décision](../stockage-bloc-v0.10.0.md) depuis la racine du monorepo. Elles utilisent Python 3, les QEMU Homebrew et des images de données temporaires exclusivement. Exécuter avec les assertions Python activées.

## Démarrage UEFI/Limine

Construire d’abord les deux ISO selon le rapport. Les deux contrôles négatifs sur Q35 laissent chacun 60 secondes au firmware pour atteindre le service v0.9.0. Le profil SATA utilise le lanceur du dépôt. Le profil SCSI déplace uniquement le CD de démarrage dans la commande de laboratoire. Chaque cas interroge QMP et relève les régions de la machine avant fermeture.

```sh
python3 - <<'PY'
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import time

root = Path('novaOS').resolve()
sys.path.insert(0, str(root / 'tests'))
from demarrage import qmp

share = Path(subprocess.check_output(['brew', '--prefix', 'qemu'], text=True).strip()) / 'share/qemu'
profiles = [('x86_64', 'scsi', 'off', True), ('x86_64', 'sata', 'off', False),
            ('x86_64', 'sans-apic', 'on', False), ('aarch64', 'auto', 'off', True),
            ('aarch64', 'off', 'off', True)]
with tempfile.TemporaryDirectory() as tmp:
    for arch, profile, boot_bypass, expected in profiles:
        work = Path(tmp)
        image, serial = work / 'data.raw', work / f'{arch}-{profile}.serie'
        image.write_bytes(bytes(1024**2))
        args = ['sh', str(root / 'scripts/qemu.sh')]
        if arch == 'aarch64':
            args += ['-machine', f'msi={profile}']
        elif profile == 'sans-apic':
            args += ['-cpu', 'max,apic=off']
        elif profile == 'scsi':
            args = ['qemu-system-x86_64', '-machine', 'q35,i8042=on,pic=on,sata=off',
                    '-cpu', 'max', '-accel', 'tcg', '-smp', '1', '-m', '256M', '-vga', 'std',
                    '-nic', 'none', '-no-reboot', '-drive',
                    f'if=pflash,unit=0,format=raw,readonly=on,file={share}/edk2-x86_64-code.fd',
                    '-drive', f'if=pflash,unit=1,format=raw,snapshot=on,file={share}/edk2-i386-vars.fd',
                    '-device', 'virtio-scsi-pci,addr=7,id=sc', '-drive',
                    f'if=none,id=cd,file={root}/build/novaos.iso,format=raw,readonly=on,media=cdrom',
                    '-device', 'scsi-cd,drive=cd,bus=sc.0,bootindex=1']
        args += ['-monitor','none','-display','none','-serial',f'file:{serial}','-qmp','stdio',
                 '-device', f'virtio-iommu-pci,id=i,addr=5,granule=4k,aw-bits=48,boot-bypass={boot_bypass}',
                 '-drive',f'if=none,id=d,file={image},format=raw,cache=writeback,werror=report,rerror=report',
                 '-device','virtio-blk-pci,id=b,addr=6,drive=d,iommu_platform=on,disable-legacy=on,ats=off,vectors=0']
        with subprocess.Popen(args, env={**os.environ,'NOVA_ARCH':arch},
                              stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0) as vm:
            try:
                qmp(vm, 'qmp_capabilities')
                assert qmp(vm, 'qom-get', path='/machine/peripheral/b', property='iommu_platform')
                assert qmp(vm, 'qom-get', path='/machine/peripheral/i', property='boot-bypass') == (boot_bypass == 'on')
                deadline = time.monotonic() + 60
                while vm.poll() is None and time.monotonic() < deadline:
                    if serial.exists() and b'INFO volumes=0 FIN confirmee' in serial.read_bytes():
                        break
                    time.sleep(.1)
                ready = serial.exists() and b'INFO volumes=0 FIN confirmee' in serial.read_bytes()
                print(arch, profile, 'service', ready, flush=True)
                assert ready == expected, 'comportement différent du banc audité'
                tree = qmp(vm, 'human-monitor-command', **{'command-line':'info mtree'})
                for line in dict.fromkeys(tree.splitlines()):
                    if any(name in line for name in ('apic-msi','gicv2m','common-virtio-blk','common-virtio-iommu')):
                        print(line.strip())
                qmp(vm, 'quit')
                assert vm.wait(5) == 0
            finally:
                if vm.poll() is None:
                    vm.kill()
                    vm.wait()
PY
```

## Traductions et persistance du périphérique émulé

Cette commande utilise le protocole QTest, avec des fonctions PCI et des pages de test réservées dans une VM sans système invité. Les valeurs de BAR et la base ECAM ARM basse appartiennent à cette topologie sans firmware. L’outil accède aux registres VirtIO, pas au gestionnaire de capacités de NovaOS.

Pour chaque ligne de la matrice, la première instance s’arrête par SIGKILL après le FLUSH du périphérique. La seconde relit la même image et vérifie la révocation. L’événement de faute doit provenir du disque et viser le secteur de la page témoin. Le témoin doit rester intact après reset. Le statut bloc du cas négatif est affiché séparément. Les messages QEMU sur la traduction absente sont attendus à cette étape. La saturation de la petite file de huit rapports limite le nombre de fautes observables.

```sh
python3 - <<'PY'
from contextlib import contextmanager
from pathlib import Path
import select
import struct
import subprocess
import tempfile
import time

PAGE = 4096
IMAGE_SIZE = 1024 * 1024
DISK_ID = 6 << 3               # 00:06.0
DOMAIN = 1
IOVA = 0x08000000
VERSION_1 = 1 << 32
ACCESS_PLATFORM = 1 << 33

class Bus:
    def __init__(self, vm, arch):
        self.vm = vm
        self.arch = arch
        self.ram = 0x02000000 if arch == 'x86_64' else 0x42000000
        self.bar = 0xf1000000 if arch == 'x86_64' else 0x10000000

    def command(self, line):
        self.vm.stdin.write((line + '\n').encode())
        if not select.select([self.vm.stdout], [], [], 5)[0]:
            raise TimeoutError(line)
        reply = self.vm.stdout.readline().decode().split()
        if reply[:1] != ['OK']:
            raise RuntimeError((line, reply))
        return reply[1] if len(reply) == 2 else None

    def reg(self, addr, size, value=None):
        suffix = {1:'b', 2:'w', 4:'l', 8:'q'}[size]
        if value is None:
            return int(self.command(f'read{suffix} {addr:#x}'), 0)
        self.command(f'write{suffix} {addr:#x} {value:#x}')

    def pci(self, slot, offset, size, value=None):
        if self.arch == 'aarch64':
            return self.reg(0x3f000000 + (slot << 15) + offset, size, value)
        self.command(f'outl 0xcf8 {0x80000000 | (slot << 11) | (offset & ~3):#x}')
        port = 0xcfc + offset % 4
        suffix = {1:'b', 2:'w', 4:'l'}[size]
        if value is None:
            return int(self.command(f'in{suffix} {port:#x}'), 0)
        self.command(f'out{suffix} {port:#x} {value:#x}')

    def put(self, addr, data):
        self.command(f'write {addr:#x} {len(data)} 0x{data.hex()}')

    def get(self, addr, size):
        return bytes.fromhex(self.command(f'read {addr:#x} {size}')[2:])


@contextmanager
def machine(arch, msi, sector, image):
    model = 'q35' if arch == 'x86_64' else f'virt,gic-version=2,highmem=off,msi={msi}'
    args = [f'qemu-system-{arch}', '-machine', model, '-accel', 'qtest', '-nodefaults',
            '-display', 'none', '-monitor', 'none', '-serial', 'none', '-m', '256M',
            '-qtest', 'stdio', '-qtest-log', '/dev/null',
            '-device', 'virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off',
            '-drive', f'if=none,id=data,file={image},format=raw,cache=writeback,rerror=report,werror=report',
            '-device', 'virtio-blk-pci,addr=6,drive=data,disable-legacy=on,iommu_platform=on,'
                       f'ats=off,vectors=0,num-queues=1,queue-size=16,logical_block_size={sector},physical_block_size={sector}']
    with subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0) as vm:
        try:
            yield Bus(vm, arch)
        finally:
            if vm.poll() is None:
                vm.kill()
            vm.wait(timeout=5)


class Device:
    def __init__(self, bus, slot, model):
        self.bus = bus
        def pci(offset, size, value=None):
            return bus.pci(slot, offset, size, value)
        assert pci(0, 4) == (model << 16) | 0x1af4
        base = bus.bar + (slot - 5) * 0x4000
        self.caps = {}
        seen = set()
        pos = pci(0x34, 1)
        while pos:
            assert 0x40 <= pos <= 0xfc and pos not in seen
            seen.add(pos)
            kind = pci(pos + 3, 1)
            if pci(pos, 1) == 9 and kind in (1, 2, 3, 4):
                assert pci(pos + 4, 1) == 4
                self.caps[kind] = base + pci(pos + 8, 4)
                if kind == 2:
                    self.stride = pci(pos + 16, 4)
            pos = pci(pos + 1, 1)
        assert set(self.caps) == {1, 2, 3, 4}
        pci(4, 2, 0)
        pci(0x24, 4, 0)
        pci(0x20, 4, base)
        pci(4, 2, 6)
        assert pci(4, 2) & 6 == 6

    def reg(self, offset, size, value=None):
        return self.bus.reg(self.caps[1] + offset, size, value)

    def config(self, offset, size):
        return self.bus.reg(self.caps[4] + offset, size)

    def reset(self):
        self.reg(20, 1, 0)
        assert self.reg(20, 1) == 0

    def start(self, wanted):
        self.reset()
        self.reg(20, 1, 1)
        self.reg(20, 1, 3)
        for bank in (0, 1):
            self.reg(0, 4, bank)
            bits = (wanted >> (bank * 32)) & 0xffffffff
            assert self.reg(4, 4) & bits == bits
            self.reg(8, 4, bank)
            self.reg(12, 4, bits)
        self.reg(20, 1, 11)
        assert self.reg(20, 1) == 11

    def ready(self):
        self.reg(20, 1, 15)
        assert self.reg(20, 1) == 15


class Queue:
    def __init__(self, device, number, physical, dma):
        self.bus, self.number, self.physical = device.bus, number, physical
        self.index = 0
        self.bus.command(f'memset {physical:#x} {3 * PAGE} 0')
        device.reg(22, 2, number)
        assert device.reg(24, 2) >= 8
        device.reg(24, 2, 8)
        self.notify = device.caps[2] + device.reg(30, 2) * device.stride
        for i, offset in enumerate((32, 40, 48)):
            addr = dma + i * PAGE
            device.reg(offset, 4, addr & 0xffffffff)
            device.reg(offset + 4, 4, addr >> 32)
        device.reg(28, 2, 1)
        assert device.reg(28, 2) == 1

    def submit(self, buffers):
        for i, (address, length, writable) in enumerate(buffers):
            more = i + 1 < len(buffers)
            descriptor = struct.pack('<QIHH', address, length, int(more) | (int(writable) << 1),
                                     i + 1 if more else 0)
            self.bus.put(self.physical + 16 * i, descriptor)
        avail = self.physical + PAGE
        self.bus.reg(avail + 4 + 2 * (self.index % 8), 2, 0)
        self.index += 1
        self.bus.reg(avail + 2, 2, self.index)
        self.bus.reg(self.notify, 2, self.number)

    def wait(self):
        used = self.physical + 2 * PAGE
        end = time.monotonic() + 2
        while self.bus.reg(used + 2, 2) != self.index:
            if time.monotonic() >= end:
                raise TimeoutError(('used', self.number, self.index))
        head, length = struct.unpack('<II', self.bus.get(used + 4 + 8 * ((self.index - 1) % 8), 8))
        assert head == 0
        return length


def exercise(bus, sector, ranges_expected, writing):
    iommu = Device(bus, 5, 0x1057)
    iommu.start(VERSION_1 | 0x57)  # INPUT_RANGE, DOMAIN_RANGE, MAP_UNMAP, PROBE, BYPASS_CONFIG
    assert iommu.config(0, 8) & PAGE and iommu.config(36, 1) == 0
    assert iommu.config(8, 8) <= IOVA and iommu.config(16, 8) >= IOVA + 6 * PAGE - 1
    assert iommu.config(24, 4) <= DOMAIN <= iommu.config(28, 4)
    requests = Queue(iommu, 0, bus.ram, bus.ram)
    events = Queue(iommu, 1, bus.ram + 3 * PAGE, bus.ram + 3 * PAGE)
    iommu.ready()

    def request(data, size=4):
        src, dst = bus.ram + 6 * PAGE, bus.ram + 7 * PAGE
        bus.put(src, data)
        bus.put(dst, bytes([0xff]) * size)
        requests.submit([(src, len(data), False), (dst, size, True)])
        assert requests.wait() == size
        result = bus.get(dst, size)
        assert result[-4] == 0, ('statut IOMMU', data[0], result[-4])
        return result[:-4]

    assert iommu.config(32, 4) == 512
    props = request(struct.pack('<II64x', 5, DISK_ID), 516)
    ranges = []
    offset = 0
    while offset + 4 <= len(props):
        kind, length = struct.unpack_from('<HH', props, offset)
        if not kind:
            break
        assert kind == 1 and length == 20
        ranges.append(struct.unpack_from('<B3xQQ', props, offset + 4))
        offset += length + 4
    assert ranges == ranges_expected, ranges
    print('PROBE', [(t, hex(lo), hex(hi)) for t, lo, hi in ranges])
    request(struct.pack('<IIII4x', 1, DOMAIN, DISK_ID, 0))  # ATTACH, domaine traduit

    physical = bus.ram + 16 * PAGE
    for n, flags in enumerate((1, 1, 3, 1, 3, 2)):
        start = IOVA + n * PAGE
        request(struct.pack('<IIQQQI', 3, DOMAIN, start, start + PAGE - 1, physical + n * PAGE, flags))
    block = Device(bus, 6, 0x1042)
    def activate():
        block.start(VERSION_1 | ACCESS_PLATFORM | (1 << 6) | (1 << 9))
        assert block.config(0, 8) == IMAGE_SIZE // 512 and block.config(20, 4) == sector
        queue = Queue(block, 0, physical, IOVA)
        block.ready()
        return queue
    queue = activate()
    def submit(op):
        bus.put(physical + 3 * PAGE, struct.pack('<IIQ', op, 0, 0 if op == 4 else 16))
        bus.reg(physical + 5 * PAGE, 1, 0xff)
        parts = [(IOVA + 3 * PAGE, 16, False)]
        if op != 4:
            parts.append((IOVA + 4 * PAGE, sector, op == 0))
        queue.submit(parts + [(IOVA + 5 * PAGE, 1, True)])
    def complete(op):
        submit(op)
        assert queue.wait() == (sector + 1 if op == 0 else 1)
        assert bus.reg(physical + 5 * PAGE, 1) == 0
        assert bus.reg(block.caps[3], 1) & 1  # interruption de file annoncée et acquittée
    if writing:
        bus.put(physical + 4 * PAGE, pattern(sector))
        complete(1)
        complete(4)
        return                              # le contexte tue cette instance de QEMU
    complete(0)
    assert bus.get(physical + 4 * PAGE, sector) == pattern(sector)

    def unmap(n):
        start = IOVA + n * PAGE
        request(struct.pack('<IIQQ4x', 4, DOMAIN, start, start + PAGE - 1))
    block.reset()
    unmap(4)
    guard = bytes([0xd2]) * PAGE
    bus.put(physical + 4 * PAGE, guard)
    event = bus.ram + 8 * PAGE
    # Huit tampons distincts permettent au contrôleur de signaler plusieurs
    # tentatives de traduction pour une seule requête. Aucun partage de tampon.
    for slot in range(8):
        hdr = struct.pack('<QIHH', event + 24 * slot, 24, 2, 0)
        bus.put(events.physical + 16 * slot, hdr)
        bus.reg(events.physical + PAGE + 4 + 2 * slot, 2, slot)
    bus.reg(events.physical + PAGE + 2, 2, 8)
    bus.reg(events.notify, 2, 1)
    queue = activate()
    submit(0)                               # IOVA de la page témoin retirée
    end = time.monotonic() + 2
    used = events.physical + 2 * PAGE
    while bus.reg(used + 2, 2) == 0:
        if time.monotonic() >= end:
            raise TimeoutError('faute matérielle attendue')
    # Le statut bloc est observé séparément de la file de fautes IOMMU.
    while bus.reg(queue.physical + 2 * PAGE + 2, 2) == 0 and time.monotonic() < end:
        time.sleep(0.002)
    completed = bus.reg(queue.physical + 2 * PAGE + 2, 2) == 1
    status = bus.reg(physical + 5 * PAGE, 1) if completed else None
    block.reset()
    count = bus.reg(used + 2, 2)
    assert 0 < count <= 8
    for slot in range(count):
        head, length = struct.unpack('<II', bus.get(used + 4 + 8 * slot, 8))
        assert head < 8 and length == 24
        reason, flags, endpoint, addr = struct.unpack('<B3xII4xQ', bus.get(event + 24 * head, 24))
        assert reason == 2 and flags & 256 and endpoint == DISK_ID
        assert IOVA + 4 * PAGE <= addr < IOVA + 4 * PAGE + sector
        if slot == 0:
            assert addr == IOVA + 4 * PAGE
    assert bus.get(physical + 4 * PAGE, PAGE) == guard
    print('Faute(s) IOMMU', count, 'statut de la requête bloc', status)
    for n in (0, 1, 2, 3, 5):
        unmap(n)
    request(struct.pack('<III8x', 2, DOMAIN, DISK_ID))  # DETACH
    print('UNMAP confirmé, SID', endpoint, 'page', hex(IOVA + 4 * PAGE), 'témoin intact')


def pattern(size):
    return bytes(0x71 + n % 83 for n in range(size))


profiles = [('x86_64', 'auto', [(1, 0xfee00000, 0xfeefffff)]),
            ('aarch64', 'auto', [(1, 0x08020000, 0x08020fff)]),
            ('aarch64', 'off', [])]
with tempfile.TemporaryDirectory() as temp:
    for arch, msi, ranges in profiles:
        for sector in (512, 4096):
            image = Path(temp) / f'{arch}-{msi}-{sector}.raw'
            image.write_bytes(bytes(IMAGE_SIZE))
            for writing in (True, False):
                print(arch, msi, sector, 'WRITE + FLUSH' if writing else 'READ + révocation')
                with machine(arch, msi, sector, image) as bus:
                    exercise(bus, sector, ranges, writing)
                assert bus.vm.returncode == -9
                expected = bytes(8192) + pattern(sector) + bytes(IMAGE_SIZE - 8192 - sector)
                assert image.read_bytes() == expected
PY
```
