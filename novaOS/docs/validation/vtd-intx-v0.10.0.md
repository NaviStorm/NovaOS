# Banc de qualification VT-d et INTx

Cette annexe reproduit les observations de la [matrice de qualification](../stockage-bloc-v0.10.0.md). Les programmes de laboratoire restent en Markdown et ne sont pas liés à NovaOS. Ils utilisent les primitives PCI, VirtIO et QTest de l’[audit précédent](dma-qemu-v0.10.0.md). Le seul vecteur MSI-X programmé dans le banc appartient au disque de test.

## Reproduction depuis la racine du monorepo

L’hôte utilisé dispose de Python 3, QEMU 11.1.3, `fdtget` (dtc), LLVM, Rust stable et des outils de construction de NovaOS. Garder les assertions Python actives. Les images et la ROM sont créées dans des répertoires temporaires. La commande ci-dessous exécute les régressions, les démarrages du noyau puis la qualification des périphériques. Chaque processus de test retourne un code non nul en cas d’écart. Les erreurs IOMMU et l’EIO volontaires sont attendues aux étapes explicitement contrôlées.

```sh
set -eu
make -C novaOS ARCH=x86_64 test
make -C novaOS ARCH=aarch64 test
banc=$(mktemp -d)
python3 - "$banc" <<'PY'
from pathlib import Path
import re
import sys

directory = Path(sys.argv[1])
docs = Path('novaOS/docs/validation')
historic = (docs/'dma-qemu-v0.10.0.md').read_text()
section = historic.split('## Traductions et persistance du périphérique émulé\n', 1)[1]
old = re.search(r"^python3 - <<'PY'\n(.*?)^PY$", section, re.M | re.S).group(1)
(directory/'historique.py').write_text(old)
(directory/'socle.py').write_text(old.split('\nprofiles = [', 1)[0] + '\n')
current = (docs/'vtd-intx-v0.10.0.md').read_text()
programs = re.findall(r'^### `([\w.]+)`\n\n```(?:python|asm)\n(.*?)^```$', current, re.M | re.S)
assert {name for name,_ in programs} == {'temoin.S','qual_vtd.py','qual_arm.py','en_vol.py','couts.py','profils.py'}
for name, source in programs:
    (directory/name).write_text(source)
PY
host=$(rustup run stable rustc -vV | awk '/^host:/ {print $2}')
sysroot=$(rustup run stable rustc --print sysroot)
"$(brew --prefix llvm)/bin/llvm-mc" -triple=i386-none-elf -filetype=obj "$banc/temoin.S" -o "$banc/temoin.o"
"$sysroot/lib/rustlib/$host/bin/rust-lld" -flavor gnu -m elf_i386 --image-base=0 -Ttext=0xf0000 --oformat=binary "$banc/temoin.o" -o "$banc/temoin.rom"
test "$(wc -c < "$banc/temoin.rom")" -eq 65536
python3 "$banc/historique.py"
python3 "$banc/profils.py"
python3 "$banc/qual_vtd.py"
python3 "$banc/qual_arm.py"
python3 "$banc/en_vol.py"
python3 "$banc/couts.py"
```

Le banc `profils.py` démarre les ISO sur le même lanceur que NovaOS. `qual_vtd.py` substitue la ROM témoin au firmware pour mesurer la livraison effective au processeur. `qual_arm.py` observe l’acquittement par l’interface CPU du GIC sous QTest. `en_vol.py` introduit une erreur récupérable sur une lecture identifiée dans une image jetable. `couts.py` mesure la durée vue de l’hôte. Une comparaison de l’image entière vérifie aussi les octets hors transfert. Les images du banc n’ont aucun rôle dans le démarrage de l’hôte.

## Témoins et charges

La version et l’empreinte de l’exécutable peuvent être vérifiées séparément sur l’hôte. Le rapport cite la révision Intel et le fichier QEMU exacts utilisés pour l’examen du traitement des formats d’interruption.

```sh
qemu-system-x86_64 --version
qemu-system-aarch64 --version
shasum -a 256 "$(command -v qemu-system-x86_64)"
```

### `temoin.S`

```asm
.section .text
.code16
.globl _start
_start:
    cli
    movb $0xff, %al
    outb %al, $0x21
    outb %al, $0xa1
    lgdtl %cs:(gdtr-_start)
    movl %cr0, %eax
    orl $1, %eax
    movl %eax, %cr0
    ljmpl $8, $flat
.code32
flat:
    movw $16, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movl $0x1ff000, %esp
    # Une seule porte externe, vecteur 81. L'IDT et le compteur
    # appartiennent au témoin CPU et sont hors des pages DMA.
    movl $interrupt, %eax
    movw %ax, 0x110288
    movw $8, 0x11028a
    movw $0x8e00, 0x11028c
    shrl $16, %eax
    movw %ax, 0x11028e
    lidtl idtr
    movl $0x1ff, 0xfee000f0
    movl $0x4e4f5641, 0x111000
attente:
    pause
    cmpl $0, 0x111004
    je attente
    sti
repos:
    hlt
    jmp repos
interrupt:
    incl 0x111008
    movl $0, 0xfee000b0
    iretl
.p2align 3
gdt:
    .quad 0, 0x00cf9a000000ffff, 0x00cf92000000ffff
gdtr:
    .word 23
    .long gdt
idtr:
    .word 2047
    .long 0x110000
.org 0xfff0
.code16
    ljmp $0xf000, $0
.org 0x10000
```

### `qual_vtd.py`

```python
"""Banc matériel : les messages du disque utilisent uniquement sa route MSI-X remappable."""
from contextlib import contextmanager
import json
from pathlib import Path
import re
import socket
import struct
import subprocess
import tempfile
import time
from socle import Bus, Device, Queue, PAGE, IMAGE_SIZE, DISK_ID, IOVA, VERSION_1, ACCESS_PLATFORM, pattern

ROM = Path(__file__).with_name('temoin.rom')
MAIL = 0x111000
VECTOR = 81
ENTRY = 4
VTD = 0xfed90000

def await_value(read, expected):
    deadline = time.monotonic() + 4
    while True:
        value = read()
        if value == expected:
            return
        if time.monotonic() >= deadline:
            raise AssertionError((expected, value))
        time.sleep(.001)

@contextmanager
def board(image, sector, arch='x86_64', on_error='report'):
    assert on_error in ('report', 'stop')
    left, right = socket.socketpair()
    x86 = arch == 'x86_64'
    if x86:
        hardware = ['-machine','q35','-accel','tcg','-cpu','max','-smp','2','-bios',str(ROM),
                    '-device','intel-iommu,intremap=on,eim=on,aw-bits=48,caching-mode=on,device-iotlb=off',
                    '-drive','if=none,id=peer,driver=null-co',
                    '-device','virtio-blk-pci,addr=7,drive=peer,disable-legacy=on,iommu_platform=on,ats=off']
    else:
        hardware = ['-machine','virt,gic-version=2,highmem=off,msi=off','-accel','qtest',
                    '-device','virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off']
    args = [f'qemu-system-{arch}',*hardware,'-m','256M','-nodefaults','-no-reboot',
            '-display','none','-serial','none','-monitor','none','-qtest','stdio','-qtest-log','/dev/null',
            '-chardev',f'socket,id=mon,fd={right.fileno()}','-mon','chardev=mon,mode=control',
            '-drive',f'if=none,id=d,file={image},format=raw,cache=writeback,rerror={on_error},werror={on_error}',
            '-device','virtio-blk-pci,id=disk,addr=6,drive=d,disable-legacy=on,iommu_platform=on,'
                      f'ats=off,vectors={2 if x86 else 0},num-queues=1,queue-size=16,'
                      f'logical_block_size={sector},physical_block_size={sector}']
    with left, right, subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                        bufsize=0, pass_fds=(right.fileno(),)) as vm:
        try:
            right.close()
            left.settimeout(5)
            with left.makefile('rwb', buffering=0) as monitor:
                assert 'QMP' in json.loads(monitor.readline())
                def qmp(op, **args):
                    monitor.write(json.dumps({'execute': op, 'arguments': args}).encode() + b'\n')
                    while True:
                        resp = json.loads(monitor.readline())
                        if 'error' in resp:
                            raise RuntimeError(resp)
                        if 'return' in resp:
                            return resp['return']
                qmp('qmp_capabilities')
                bus = Bus(vm,arch)
                if x86:
                    await_value(lambda: bus.reg(MAIL,4), 0x4e4f5641)
                yield bus, qmp
        finally:
            if vm.poll() is None:
                vm.kill()
            vm.wait(5)

class Remapper:
    def __init__(self, bus):
        self.bus = bus
        base = bus.ram
        self.leaves = base + 5 * PAGE
        self.ir = base + 6 * PAGE
        self.iq = base + 7 * PAGE
        self.done = base + 8 * PAGE
        self.tail = 0
        self.serial = 0
        self.cap = self.reg(8,8)
        ecap = self.reg(16,8)
        assert ecap & 0x1a == 0x1a and self.cap & (1 << 10)
        self.fault_base = VTD + ((self.cap >> 24) & 0x3ff) * 16
        bus.reg(base,8, (base + PAGE) | 1)
        ctx = base + PAGE + 16 * DISK_ID
        bus.put(ctx, struct.pack('<QQ', (base + 2 * PAGE) | 1, (1 << 8) | 2))
        for n, shift in enumerate((39,30,21)):
            bus.reg(base + (2+n)*PAGE + 8*((IOVA>>shift)&511),8,(base + (3+n)*PAGE) | 3)
        for n, flags in enumerate((1,1,3,1,3,2)):
            self.leaf(n, base + (16+n)*PAGE, flags)
        self.reg(0x20,8,base)                   # registre de table racine
        self.reg(0x18,4,1<<30)
        self.reg(0xb8,8,self.ir | 0x800 | 3)   # EIME, seize IRTE
        self.reg(0x18,4,1<<24)
        self.reg(0x90,8,self.iq)
        self.reg(0x18,4,(1<<31) | (1<<26) | (1<<25))
        assert self.reg(0xb8,8) == self.ir | 0x803
        assert self.reg(0x1c,4) & 0xc7800000 == 0xc7000000
        self.invalidate([0x11,0x12,4])           # contexte, IOTLB, cache des IRTE
        print(f'ECAP={ecap:#x} GSTS={self.reg(0x1c,4):#x} IRTA={self.reg(0xb8,8):#x}')

    def reg(self, offset, size, value=None):
        return self.bus.reg(VTD+offset,size,value)

    def leaf(self, n, physical, flags):
        self.bus.reg(self.leaves + 8*(((IOVA >> 12) & 511)+n),8,physical | flags)

    def invalidate(self, items):
        self.serial += 1
        self.bus.reg(self.done,4,0)
        descriptors = [(item,0) for item in items]
        descriptors.append(((self.serial << 32) | 0x25, self.done))   # WAIT avec écriture d'état
        for low, high in descriptors:
            self.bus.put(self.iq + self.tail, struct.pack('<QQ',low,high))
            self.tail = (self.tail+16) % PAGE
        self.reg(0x88,8,self.tail)
        await_value(lambda: self.bus.reg(self.done,4), self.serial)
        assert self.reg(0x80,8) == self.tail

    def route(self, owner):
        low = (VECTOR << 16) | 1 if owner is not None else 0
        high = (1 << 18) | owner if owner is not None else 0           # SVT exact, SQ nul
        self.bus.put(self.ir + ENTRY*16, struct.pack('<QQ',low,high))
        self.invalidate([4])

    def fault(self, label):
        high = self.bus.reg(self.fault_base+8,8)
        low = self.bus.reg(self.fault_base,8)
        reason = (high >> 32) & 255
        assert high >> 63 and high & 65535 == DISK_ID and reason, (label,hex(low),hex(high))
        self.bus.reg(self.fault_base+12,4,1<<31)
        assert not self.bus.reg(self.fault_base+8,8) >> 63
        print(label,'SID',high & 65535,'motif',hex(reason),'information',hex(low))
        return low, reason

def setup_msix(bus, device):
    slot = 6
    cfg = lambda offset,size,val=None: bus.pci(slot,offset,size,val)
    cap = cfg(0x34,1)
    seen = set()
    while cap and cfg(cap,1) != 0x11:
        assert cap not in seen
        seen.add(cap)
        cap = cfg(cap+1,1)
    assert cap and cfg(cap+4,4) & 7 == 1
    base = 0xf1100000
    cfg(4,2,0)
    cfg(0x14,4,base)
    cfg(4,2,0x406)                            # bus master, MMIO, INTx masqué
    ctrl = cfg(cap+2,2)
    cfg(cap+2,2,ctrl | 0xc000)
    addr = base + (cfg(cap+4,4) & ~7)
    bus.reg(addr+12,4,1)
    # Le banc programme exclusivement le handle remappable attribué au disque.
    bus.reg(addr,4,0xfee00010 | (ENTRY << 5))
    bus.reg(addr+4,4,0)
    bus.reg(addr+8,4,0)
    bus.reg(addr+12,4,0)
    cfg(cap+2,2,(ctrl | 0x8000) & ~0x4000)
    device.reg(26,2,0)
    assert device.reg(26,2) == 0
    pba = cfg(cap+8,4)
    assert pba & 7 == 1
    return addr+12, base+(pba & ~7)

def run(bus, qmp, size, writing):
    iommu = Remapper(bus)
    peer = bus.pci(7,0,4)
    assert peer == 0x10421af4
    device = Device(bus,6,0x1042)
    physical = bus.ram + 16*PAGE
    src = physical + 3*PAGE
    buf = physical + 4*PAGE
    status = physical + 5*PAGE
    msix = None
    def activate():
        nonlocal msix
        device.start(VERSION_1 | ACCESS_PLATFORM | (1<<6) | (1<<9))
        assert device.config(20,4)==size and device.config(0,8)==IMAGE_SIZE//512
        queue = Queue(device,0,physical,IOVA)
        msix = setup_msix(bus,device)
        device.ready()
        return queue
    def submit(op, valid=True):
        bus.put(src,struct.pack('<IIQ',op,0,0 if op==4 else 16))
        bus.reg(status,1,255)
        segs=[(IOVA+3*PAGE,16,False)]
        if op!=4: segs.append((IOVA+4*PAGE,size,op==0))
        queue.submit(segs+[(IOVA+5*PAGE,1,True)])
        if valid:
            assert queue.wait()==(size+1 if op==0 else 1) and bus.reg(status,1)==0
    def pending(cpu):
        result = qmp('human-monitor-command', **{'command-line':f'info lapic {cpu}'})
        line = re.search(r'^IRR\s+([^\r\n]*)',result,re.M)
        assert line, result
        return str(VECTOR) in line[1].split()
    iommu.route(7<<3)                           # l'administrateur attribue l'entrée au second témoin
    queue = activate()
    submit(4)
    _, wrong_source = iommu.fault('source exclue')
    assert wrong_source==0x26
    assert not pending(0) and not pending(1)
    iommu.route(DISK_ID)
    if writing:
        bus.put(buf,pattern(size))
        submit(1)
    else:
        submit(0)
        assert bus.get(buf,size)==pattern(size)
    assert pending(0) and not pending(1) and bus.reg(MAIL+8,4)==0
    iommu.route(None)                           # retirer la route déjà acceptée par le LAPIC
    assert pending(0)
    bus.reg(MAIL+4,4,1)                          # le témoin accepte les IRQ et renvoie EOI
    await_value(lambda:bus.reg(MAIL+8,4),1)
    submit(4)
    _, withdrawn = iommu.fault('route retirée')
    assert withdrawn==0x22
    assert not pending(0) and not pending(1) and bus.reg(MAIL+8,4)==1
    iommu.route(DISK_ID)
    submit(4)
    await_value(lambda:bus.reg(MAIL+8,4),2)
    control, pba = msix
    bus.reg(control,4,1)
    submit(4)
    assert bus.reg(pba,4)&1 and bus.reg(MAIL+8,4)==2 and not pending(0)
    iommu.route(None)
    device.reset()
    buffered = bus.reg(pba,4)&1
    print('MSI-X PBA avant reset=1, après reset=',buffered)
    bus.reg(control,4,0)           # route fermée pendant la purge de la source
    assert not bus.reg(pba,4)&1 and bus.reg(MAIL+8,4)==2 and not pending(0)
    if bus.reg(iommu.fault_base+8,8)>>63:
        _, closed = iommu.fault('purge sur route fermée')
        assert closed==0x22
    if writing:
        return
    iommu.route(DISK_ID)
    device.reset()
    iommu.leaf(4,0,0)
    iommu.invalidate([0x12])
    witness = bytes([0xd2])*PAGE
    bus.put(buf,witness)
    queue = activate()
    submit(0,valid=False)
    await_value(lambda:bool(bus.reg(iommu.fault_base+8,8)>>63),True)
    low, reason=iommu.fault('page retirée')
    assert reason == 0x05 and low & ~(PAGE-1)==IOVA+4*PAGE
    device.reset()
    assert bus.get(buf,PAGE)==witness
    # Un nouveau cadre reçoit la même IOVA. Le cadre rendu reste intact.
    new = physical+6*PAGE
    bus.put(new,bytes(PAGE))
    iommu.leaf(4,new,3)
    iommu.invalidate([0x12])
    queue = activate()
    submit(0)
    assert bus.get(new,size)==pattern(size) and bus.get(buf,PAGE)==witness
    device.reset()
    bus.pci(6,4,2,0)
    iommu.route(None)
    bus.put(bus.ram+PAGE+16*DISK_ID,bytes(16))
    iommu.invalidate([0x11,0x12])
    print('Ancien cadre intact après reset et réemploi de l’IOVA.')
    # Opération d'invalidation vide (type réservé), suivie d'un WAIT témoin.
    # Le périphérique est arrêté et son domaine retiré. Le banc garde le cadre.
    head = iommu.reg(0x80,8)
    assert head == iommu.tail
    bus.reg(iommu.done,4,0)
    bus.put(iommu.iq+head,bytes(16))
    bus.put(iommu.iq+((head+16)%PAGE),struct.pack('<QQ',(1<<32)|0x25,iommu.done))
    iommu.reg(0x88,8,(head+32)%PAGE)
    await_value(lambda:bool(iommu.reg(0x34,4)&0x10),True)
    assert iommu.reg(0x80,8)==head and bus.reg(iommu.done,4)==0
    assert bus.get(buf,PAGE)==witness
    print('IQE=1, IQH arrêté, WAIT absent, cadre conservé jusqu’à la destruction de la VM.')

if __name__=='__main__':
    with tempfile.TemporaryDirectory() as tmp:
        for size in (512,4096):
            image=Path(tmp)/f'{size}.raw'
            image.write_bytes(bytes(IMAGE_SIZE))
            for first in (True,False):
                print('Q35',size,'écriture' if first else 'relance',flush=True)
                with board(image,size) as (bus,qmp):
                    run(bus,qmp,size,first)
                assert bus.vm.returncode == -9
                assert image.read_bytes()==bytes(8192)+pattern(size)+bytes(IMAGE_SIZE-8192-size)
```

### `qual_arm.py`

```python
"""INTx câblé de la fonction bloc 00:06.0, GICv2, topologie fixe du banc."""
from pathlib import Path
import tempfile
import struct
from socle import (Bus, Device, Queue, PAGE, IOVA, DISK_ID, VERSION_1, ACCESS_PLATFORM,
                   IMAGE_SIZE, exercise, machine, pattern)

DIST = 0x08000000
CPU = 0x08010000
IRQ = 37
MASK = 1 << (IRQ % 32)
SPURIOUS = 1023

class GicBus(Bus):
    def __init__(self, bus):
        super().__init__(bus.vm, 'aarch64')
        self.completions = 0
        self.isr = None
        slot = 6
        assert self.pci(slot, 0x3d, 1) == 1       # INTA
        cap = self.pci(slot, 0x34, 1)
        visited = set()
        while cap:
            assert cap not in visited
            visited.add(cap)
            kind = self.pci(slot, cap, 1)
            assert kind not in (0x05, 0x11)        # ni MSI ni MSI-X du disque
            if kind == 9 and self.pci(slot, cap+3, 1) == 3:
                assert self.pci(slot, cap+4, 1) == 4
                self.isr = self.bar + 0x4000 + self.pci(slot, cap+8, 4)
            cap = self.pci(slot, cap+1, 1)
        assert self.isr is not None
        self.reg(DIST+0x400+IRQ, 1, 0x40)
        self.reg(DIST+0x800+IRQ, 1, 1)
        self.reg(DIST+0x104, 4, MASK)
        self.reg(DIST, 4, 1)
        self.reg(CPU+4, 4, 0xff)
        self.reg(CPU, 4, 1)
        assert self.reg(CPU+0x18, 4) == SPURIOUS

    def reg(self, addr, size, value=None):
        read = super().reg
        if self.isr is None or addr != self.isr or value is not None:
            return read(addr, size, value)
        self.completions += 1
        assert read(DIST+0x204, 4) & MASK
        assert read(CPU+0x18, 4) == IRQ
        if self.completions == 2:
            read(DIST+0x184, 4, MASK)             # retirer la route niveau déjà en attente
            assert not read(DIST+0x104, 4) & MASK
            assert read(CPU+0x0c, 4) == SPURIOUS and not read(DIST+0x304, 4) & MASK
            result = read(addr, size)             # acquitter la source, puis purger le GIC
            read(DIST+0x284, 4, MASK)
            read(DIST+0x104, 4, MASK)
            assert not read(DIST+0x204, 4) & MASK and read(CPU+0x18, 4) == SPURIOUS
            print('Ligne masquée, IAR=1023, source purgée avant réarmement.')
            return result
        token = read(CPU+0x0c, 4)
        assert token == IRQ and read(DIST+0x304, 4) & MASK
        result = read(addr, size)                 # le périphérique relâche la ligne INTx
        read(CPU+0x10, 4, token)
        assert not read(DIST+0x304, 4) & MASK and read(CPU+0x18, 4) == SPURIOUS
        print('INTx IAR=37, actif, EOI, HPPIR=1023.')
        return result

def nouveau_domaine(bus, size):
    # La première instance de domaine a fait RESET, UNMAP et DETACH.
    physical = bus.ram + 16 * PAGE
    old = physical + 4 * PAGE
    new = physical + 6 * PAGE
    guard = bytes([0xd2]) * PAGE
    assert bus.get(old, PAGE) == guard
    disk = Device(bus, 6, 0x1042)
    disk.reset()
    bus.reg(DIST+0x184, 4, MASK)
    bus.reg(disk.caps[3], 1)
    bus.reg(DIST+0x284, 4, MASK)
    iommu = Device(bus, 5, 0x1057)
    iommu.start(VERSION_1 | 0x57)
    assert iommu.config(36, 1) == 0
    requests = Queue(iommu, 0, bus.ram, bus.ram)
    events = Queue(iommu, 1, bus.ram + 3 * PAGE, bus.ram + 3 * PAGE)
    iommu.ready()
    def req(data):
        src, dst = bus.ram + 6 * PAGE, bus.ram + 7 * PAGE
        bus.put(src, data)
        bus.put(dst, bytes([0xff]) * 4)
        requests.submit([(src, len(data), False), (dst, 4, True)])
        assert requests.wait() == 4
        return bus.get(dst, 4)[0]
    domain = 2
    assert req(struct.pack('<IIII4x', 1, domain, DISK_ID, 0)) == 0
    # Invalidation d'un domaine inexistant et opération non prise en charge.
    missing = req(struct.pack('<IIQQ4x', 4, 3, IOVA, IOVA + PAGE-1))
    invalid = req(struct.pack('<I16x', 0xfe))
    assert missing == 6 and invalid == 2, (missing, invalid)
    for n, flags in enumerate((1, 1, 3, 1, 3, 2)):
        first = IOVA+n*PAGE
        dest = new if n == 4 else physical+n*PAGE
        assert req(struct.pack('<IIQQQI', 3, domain, first, first+PAGE-1, dest, flags)) == 0
    bus.put(new, bytes(PAGE))
    disk.start(VERSION_1 | ACCESS_PLATFORM | (1<<6) | (1<<9))
    queue = Queue(disk, 0, physical, IOVA)
    disk.ready()
    bus.put(physical+3*PAGE, struct.pack('<IIQ', 0, 0, 16))
    bus.reg(physical+5*PAGE, 1, 255)
    queue.submit([(IOVA+3*PAGE, 16, False), (IOVA+4*PAGE, size, True), (IOVA+5*PAGE, 1, True)])
    assert queue.wait() == size+1 and bus.reg(physical+5*PAGE, 1) == 0
    assert bus.get(new, PAGE) == pattern(size) + bytes(PAGE-size) and bus.get(old, PAGE) == guard
    assert bus.reg(DIST+0x204, 4) & MASK and bus.reg(CPU+0x0c, 4) == SPURIOUS
    assert bus.reg(disk.caps[3], 1) & 1
    disk.reset()
    bus.pci(6, 4, 2, 0)
    for n in range(6):
        first = IOVA+n*PAGE
        assert req(struct.pack('<IIQQ4x', 4, domain, first, first+PAGE-1)) == 0
    assert req(struct.pack('<III8x', 2, domain, DISK_ID)) == 0
    bus.reg(DIST+0x284, 4, MASK)
    bus.reg(DIST+0x104, 4, MASK)
    assert bus.reg(CPU+0x18, 4) == SPURIOUS
    print('Domaine neuf, même IOVA, ancien cadre intact. NOENT', missing, 'UNSUPP', invalid)


if __name__ == '__main__':
    with tempfile.TemporaryDirectory() as tmp:
        for size in (512, 4096):
            image = Path(tmp)/f'{size}.raw'
            image.write_bytes(bytes(IMAGE_SIZE))
            for writing in (True, False):
                print('virt sans MSI', size, 'écriture' if writing else 'relance', flush=True)
                with machine('aarch64', 'off', size, image) as bus:
                    probe = GicBus(bus)
                    exercise(probe, size, [], writing)
                    assert probe.completions == (2 if writing else 1)
                    if not writing:
                        nouveau_domaine(bus, size)
                assert bus.vm.returncode == -9
                assert image.read_bytes() == bytes(8192) + pattern(size) + bytes(IMAGE_SIZE-8192-size)
```

### `en_vol.py`

```python
"""Quatre FLUSH ordinaires dans l'anneau complet. READ retenu sur EIO de l'image de test."""
from pathlib import Path
import struct
import tempfile
from socle import Device, Queue, VERSION_1, ACCESS_PLATFORM, IOVA, PAGE, IMAGE_SIZE
from qual_vtd import board, await_value, setup_msix
from couts import materiel

def serie(arch, sector, bus, qmp):
    changer, router = materiel(arch,bus)
    base = bus.ram+16*PAGE
    disk = Device(bus,6,0x1042)
    disk.start(VERSION_1 | ACCESS_PLATFORM | (1<<6) | (1<<9))
    q = Queue(disk,0,base,IOVA)
    if arch == 'x86_64':
        setup_msix(bus,disk)
        bus.reg(0x111004,4,1)
    disk.ready()
    # Tous les descripteurs octroyés à la file sont en cours, chacun a son propre statut.
    for i in range(4):
        bus.put(base+3*PAGE+16*i,struct.pack('<IIQ',4,0,0))
        bus.reg(base+5*PAGE+i,1,0xff)
        pair = struct.pack('<QIHH',IOVA+3*PAGE+16*i,16,1,2*i+1)
        pair += struct.pack('<QIHH',IOVA+5*PAGE+i,1,2,0)
        bus.put(base+32*i,pair)
        bus.reg(base+PAGE+4+2*i,2,2*i)
    bus.reg(base+PAGE+2,2,4)
    bus.reg(q.notify,2,0)
    await_value(lambda:bus.reg(base+2*PAGE+2,2),4)
    heads = sorted(struct.unpack('<II',bus.get(base+2*PAGE+4+8*i,8)) for i in range(4))
    assert heads == [(i,1) for i in (0,2,4,6)] and bus.get(base+5*PAGE,4)==bytes(4)
    q.index=4
    # EIO injecté par blkdebug sur READ à l'offset 8192. QEMU retient le transfert.
    witness = bytes([0xe1])*PAGE
    bus.put(base+4*PAGE,witness)
    bus.put(base+3*PAGE,struct.pack('<IIQ',0,0,16))
    bus.reg(base+5*PAGE,1,0xff)
    q.submit([(IOVA+3*PAGE,16,False),(IOVA+4*PAGE,sector,True),(IOVA+5*PAGE,1,True)])
    def status():
        items = qmp('query-block')
        return next(item.get('io-status','') for item in items if item['device']=='d')
    await_value(status,'failed')
    await_value(lambda:qmp('query-status')['status'],'io-error')
    assert bus.reg(base+2*PAGE+2,2)==4 and bus.reg(base+5*PAGE,1)==0xff
    assert bus.get(base+4*PAGE,PAGE)==witness
    disk.reset()
    bus.pci(6,4,2,0)
    router(False)
    changer(4,0,0)
    replacement = bytes([0xc3])*PAGE
    bus.put(base+4*PAGE,replacement)
    qmp('cont')
    # Une nouvelle requête de lecture réussie prouve que le backend a réellement repris.
    new = base+6*PAGE
    bus.put(new,bytes(PAGE))
    changer(4,new,3)
    disk = Device(bus,6,0x1042)
    disk.start(VERSION_1 | ACCESS_PLATFORM | (1<<6) | (1<<9))
    q = Queue(disk,0,base,IOVA)
    if arch == 'x86_64': setup_msix(bus,disk)
    router(True)
    disk.ready()
    bus.put(base+3*PAGE,struct.pack('<IIQ',0,0,16))
    bus.reg(base+5*PAGE,1,0xff)
    q.submit([(IOVA+3*PAGE,16,False),(IOVA+4*PAGE,sector,True),(IOVA+5*PAGE,1,True)])
    assert q.wait()==sector+1 and bus.reg(base+5*PAGE,1)==0
    assert bus.get(new,PAGE)==bytes([0x39])*sector+bytes(PAGE-sector)
    assert bus.get(base+4*PAGE,PAGE)==replacement
    disk.reset()
    print(arch,sector,'8/8 descripteurs, io-error en vol, reset, ancien cadre intact, reprise vérifiée.')

with tempfile.TemporaryDirectory() as tmp:
    cfg = Path(tmp)/'inject.conf'
    cfg.write_text('[inject-error]\nevent = "read_aio"\nerrno = "5"\nsector = "16"\nonce = "on"\n')
    for arch in ('x86_64','aarch64'):
        for sector in (512,4096):
            image = Path(tmp)/f'{arch}-{sector}.raw'
            expected = bytes(8192)+bytes([0x39])*sector+bytes(IMAGE_SIZE-8192-sector)
            image.write_bytes(expected)
            with board(f'blkdebug:{cfg}:{image}',sector,arch,on_error='stop') as (bus,qmp):
                serie(arch,sector,bus,qmp)
            assert image.read_bytes()==expected
```

### `couts.py`

```python
"""Temps hôte d'un initiateur QTest de profondeur 1. Horloge monotone en ns."""
from contextlib import contextmanager
from pathlib import Path
import math
import random
import struct
import tempfile
from time import perf_counter_ns

from socle import (Device, Queue, PAGE, IMAGE_SIZE, DISK_ID, IOVA, VERSION_1,
                   ACCESS_PLATFORM, machine)
from qual_vtd import board, Remapper, setup_msix
from qual_arm import DIST, MASK

def statistiques(label, samples, throughput=None):
    times = sorted(samples)
    def percentile(p):
        return times[math.ceil(len(times)*p)-1]/1000
    print(label, len(times), *(f'{percentile(p):.1f}' for p in (.50,.99,1)),
          '' if throughput is None else f'{throughput:.2f}')

@contextmanager
def plate(arch, image, sector):
    if arch == 'x86_64':
        with board(image, sector) as (bus, _):
            yield bus
    else:
        with machine(arch, 'off', sector, image) as bus:
            yield bus

def materiel(arch, bus):
    if arch == 'x86_64':
        unit = Remapper(bus)
        def changer(n, frame, flags):
            unit.leaf(n, frame, flags)
            unit.invalidate([0x12])
        unit.route(DISK_ID)
        return changer, lambda enable: unit.route(DISK_ID if enable else None)

    unit = Device(bus, 5, 0x1057)
    unit.start(VERSION_1 | 0x57)
    assert unit.config(36, 1) == 0
    control = Queue(unit, 0, bus.ram, bus.ram)
    events = Queue(unit, 1, bus.ram+3*PAGE, bus.ram+3*PAGE)
    unit.ready()
    def command(data):
        src, dst = bus.ram+6*PAGE, bus.ram+7*PAGE
        bus.put(src, data)
        bus.reg(dst, 4, 0xffffffff)
        control.submit([(src, len(data), False), (dst, 4, True)])
        assert control.wait() == 4 and bus.reg(dst, 1) == 0
    command(struct.pack('<IIII4x', 1, 1, DISK_ID, 0))
    def changer(n, frame, flags):
        start = IOVA+n*PAGE
        data = (struct.pack('<IIQQQI',3,1,start,start+PAGE-1,frame,flags) if flags else
                struct.pack('<IIQQ4x',4,1,start,start+PAGE-1))
        command(data)
    for n, flags in enumerate((1,1,3,1,3,2)):
        changer(n, bus.ram+(16+n)*PAGE, flags)
    def router(enable):
        bus.reg(DIST+(0x104 if enable else 0x184), 4, MASK)
        assert bool(bus.reg(DIST+0x104, 4) & MASK) == enable
    return changer, router

def campagne(arch, bus, image, sector, oracle):
    changer, router = materiel(arch, bus)
    base = bus.ram + 16*PAGE
    disk = Device(bus, 6, 0x1042)
    disk.start(VERSION_1 | ACCESS_PLATFORM | (1<<6) | (1<<9))
    assert disk.config(20,4) == sector
    queue = Queue(disk, 0, base, IOVA)
    if arch == 'x86_64':
        setup_msix(bus, disk)
        bus.reg(0x111004,4,1)
    disk.ready()
    def chrono(action):
        start = perf_counter_ns()
        action()
        return perf_counter_ns()-start
    timings = [[] for _ in range(4)]
    for _ in range(32):
        timings[0].append(chrono(lambda: changer(6,base+6*PAGE,3)))
        timings[1].append(chrono(lambda: changer(6,0,0)))
        timings[2].append(chrono(lambda: router(False)))
        timings[3].append(chrono(lambda: router(True)))
    for label, values in zip(('MAP','UNMAP','IRQ_OFF','IRQ_ON'), timings):
        statistiques(label, values)
    def operation(op, page):
        bus.put(base+3*PAGE, struct.pack('<IIQ',op,0,page*(PAGE//512)))
        bus.reg(base+5*PAGE,1,0xff)
        parts = [(IOVA+3*PAGE,16,False)]
        if op != 4:
            parts.append((IOVA+4*PAGE,PAGE,op == 0))
        parts.append((IOVA+5*PAGE,1,True))
        def issue():
            queue.submit(parts)
            assert queue.wait() == (PAGE+1 if op==0 else 1) and bus.reg(base+5*PAGE,1)==0
        return chrono(issue)
    for shuffled in (False, True):
        order = list(range(128))
        if shuffled:
            random.Random(381).shuffle(order)
        for op in (1, 0):
            samples = []
            begin = perf_counter_ns()
            for page in order:
                span = slice(page*PAGE,(page+1)*PAGE)
                if op == 1:
                    data = struct.pack('<II', page, int(shuffled)) * (PAGE//8)
                    oracle[span] = data
                    bus.put(base+4*PAGE, data)
                samples.append(operation(op,page))
                if op == 0:
                    assert bus.get(base+4*PAGE,PAGE) == oracle[span]
            rate = len(order)*PAGE/(1024**2) / ((perf_counter_ns()-begin)*1e-9)
            statistiques(('W' if op else 'R')+('_ALEA' if shuffled else '_SEQ'), samples, rate)
            if op == 1:
                statistiques('FLUSH', [operation(4,0)])
    statistiques('FLUSH_SANS_ECRITURE', [operation(4,0) for _ in range(64)])
    disk.reset()

if __name__ == '__main__':
    with tempfile.TemporaryDirectory() as tmp:
        for arch in ('x86_64','aarch64'):
            for sector in (512,4096):
                image = Path(tmp)/f'{arch}-{sector}.raw'
                oracle = bytearray(IMAGE_SIZE)
                image.write_bytes(oracle)
                print(arch, sector, 'n p50_us p99_us max_us Mio/s', flush=True)
                with plate(arch, image, sector) as bus:
                    campagne(arch, bus, image, sector, oracle)
                assert image.read_bytes() == oracle
```

### `profils.py`

```python
"""Démarrage du noyau v0.9.0 avec les topologies candidates et des disques jetables."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile

root = Path('novaOS').resolve()
sys.path.insert(0, str(root/'tests'))
from demarrage import qmp, wait_demo, wait_storage

def routes_arm(work):
    routes = None
    for highmem in ('off','on'):
        dtb = Path(work)/f'virt-{highmem}.dtb'
        subprocess.run(['qemu-system-aarch64','-machine',
                        f'virt,gic-version=2,msi=off,highmem={highmem},dumpdtb={dtb}',
                        '-cpu','cortex-a72','-m','256M','-nodefaults','-display','none',
                        '-device','virtio-scsi-pci,addr=1','-device',
                        'virtio-iommu-pci,addr=5,granule=4k,aw-bits=48,boot-bypass=off','-drive',
                        'if=none,id=d,driver=null-co','-device',
                        'virtio-blk-pci,addr=6,drive=d,vectors=0,disable-legacy=on,iommu_platform=on,ats=off'],
                       check=True)
        node = '/pcie@10000000'
        def cells(prop):
            value = subprocess.check_output(['fdtget','-tx',dtb,node,prop],text=True)
            return [int(x,16) for x in value.split()]
        assert cells('interrupt-map-mask') == [0x1800,0,0,7]
        assert 'msi-parent' not in subprocess.check_output(['fdtget','-p',dtb,node],text=True).split()
        raw = cells('interrupt-map')
        assert len(raw)==160
        table = {}
        for pos in range(0,len(raw),10):
            addr, _, _, pin, _, _, _, kind, spi, flags = raw[pos:pos+10]
            assert kind==0 and flags==4
            table[(addr>>11,pin)]=32+spi
        assert routes is None or table==routes
        routes=table
    return routes

with tempfile.TemporaryDirectory() as work:
    routes = routes_arm(work)
    for arch in ('x86_64', 'aarch64'):
        for sector in (512, 4096):
            image, log = (Path(work)/f'{arch}-{sector}.{ext}' for ext in ('raw','serie'))
            image.write_bytes(bytes(1024 * 1024))
            arm = arch == 'aarch64'
            options = (['-machine', 'msi=off', '-device',
                        'virtio-iommu-pci,id=i,addr=5,granule=4k,aw-bits=48,boot-bypass=off'] if arm
                       else ['-device', 'intel-iommu,id=i,intremap=on,eim=on,aw-bits=48,caching-mode=on,device-iotlb=off'])
            args = ['sh', root/'scripts/qemu.sh', '-display','none','-monitor','none',
                    '-serial',f'file:{log}','-qmp','stdio', *options, '-drive',
                    f'if=none,id=d,file={image},format=raw,cache=writeback,werror=report,rerror=report',
                    '-device',f'virtio-blk-pci,id=b,addr=6,drive=d,disable-legacy=on,iommu_platform=on,'
                              f'ats=off,vectors={0 if arm else 2},logical_block_size={sector},physical_block_size={sector}']
            with subprocess.Popen(args,env={**os.environ,'NOVA_ARCH':arch},
                                  stdin=subprocess.PIPE,stdout=subprocess.PIPE,bufsize=0) as vm:
                try:
                    qmp(vm,'qmp_capabilities')
                    wait_demo(vm,log,'EL0' if arm else 'Ring3')
                    wait_storage(vm,log)
                    devices = qmp(vm,'query-pci')[0]['devices']
                    assert any(d['qdev_id']=='b' and d['slot']==6 and d['function']==0 for d in devices)
                    path = '/machine/peripheral/b'
                    assert qmp(vm,'qom-get',path=path,property='iommu_platform')
                    assert not qmp(vm,'qom-get',path=path,property='ats')
                    assert qmp(vm,'qom-get',path=path,property='logical_block_size')==sector
                    assert qmp(vm,'qom-get',path=path,property='physical_block_size')==sector
                    if arm:
                        assert not qmp(vm,'qom-get',path='/machine/peripheral/i',property='boot-bypass')
                        assert qmp(vm,'qom-get',path=path,property='vectors')==0
                        pins = [(d['slot'],d['irq_pin']) for d in devices if d.get('irq_pin',0)]
                        owners = [(slot,routes[slot % 4,pin]) for slot,pin in pins]
                        assert (6,37) in owners and [slot for slot,irq in owners if irq==37]==[6], owners
                        print('ARM lignes câblées',owners)
                    print(arch, sector, 'INFO volumes=0', [(d['slot'],d['id']['device']) for d in devices], flush=True)
                    qmp(vm,'quit')
                    assert vm.wait(5)==0
                finally:
                    if vm.poll() is None:
                        vm.kill()
                        vm.wait()
            assert image.read_bytes()==bytes(1024 * 1024)
```
