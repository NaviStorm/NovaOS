"""Reconstitue les vecteurs avec des graines publiques de test et cryptography."""
from pathlib import Path
from hashlib import sha256
import struct
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

OUT = Path(__file__).resolve().parent
def key(n):
    return Ed25519PrivateKey.from_private_bytes(bytes([n])*32)

def envelope(label, data):
    return label.encode()+b'\0'+struct.pack('<I', len(data))+data

def elf(machine):
    # Une instruction de retour dans un PT_LOAD RX. Cette image ne sera pas lancée.
    code = bytes.fromhex("c0035fd6" if machine == 183 else "c3")
    entry = 0x400000 + 120
    header = b'\x7fELF\x02\x01\x01'+bytes(9)+struct.pack('<HHIQQQIHHHHHH',2,machine,1,entry,64,0,0,64,56,1,0,0,0)
    segment = struct.pack('<IIQQQQQQ',1,5,0,0x400000,0,120+len(code),120+len(code),4096)
    return header+segment+code

def write(name, data):
    (OUT/name).write_bytes(data)
    return sha256(data).hexdigest()

for name,n in [('pilote',1),('elf',2),('systeme',3)]:
    write(name+'.pub', key(n).public_key().public_bytes_raw())
obj=b'profil de mediation fictif\n'
profile_hash=write('profil.bin',obj)
record=f'origin="laboratoire"\npolicy_revision=1\nsecurity_epoch=3\nid="org.novaos.lab.courtier"\nkind="system-profile"\nversion=[1,0,0]\nsha256="{profile_hash}"\ndependencies=[]\n'.encode()
write('profil.toml',record)
write('profil.sig',key(3).sign(envelope('NovaOS/driver-resource/v1',record)))
images=[]
for isa,machine in [('aarch64',183),('x86_64',62)]:
    image=elf(machine)
    member=b'pilote'
    manifest=b'NovaOS/ELF/pkg01'+bytes([0x42])*16+struct.pack('<IHH',1,machine,1)+bytes([len(member)])+member+struct.pack('<I',len(image))+sha256(image).digest()
    package=struct.pack('<I',len(manifest))+manifest+key(2).sign(manifest)+image
    package_hash=write(isa+'.pkg01',package)
    images.append(f'[[image]]\nisa="{isa}"\nmember="pilote"\npackage_sha256="{package_hash}"\nelf_sha256="{sha256(image).hexdigest()}"\n')
raw=f'''schema="novaos.driver/1"
id="org.novaos.lab.port"
version=[1,0,0]
security_epoch=3
origin="laboratoire"
publisher="org.novaos.lab"
signing_key_id="lab-v1"
elf_package_id="{'42'*16}"
elf_package_revision=1
class="ethernet"
provides=["ethernet/1.0"]
required_caps=["rx"]
optional_caps=["stats"]

[compat]
architectures=["aarch64","x86_64"]
abi_major=1
os_min=[0,20,0]
os_before=[0,21,0]
required_features=["courtier"]

{chr(10).join(images)}
[[match]]
bus="pci"
vendor=0x1af4
device=0x1041

[[dependency]]
id="org.novaos.lab.courtier"
kind="system-profile"
min=[1,0,0]
before=[2,0,0]
sha256="{profile_hash}"
required=true

[hardware]
dma=["rx:w"]
mmio=["notification"]
irq=["intx"]
max_irq_routes=1
dma_pages=22
queues=2
entries_per_queue=16

[quotas]
private_pages=32
ipc_roots_created=1
ipc_handles=6
ipc_boxes_created=0
pending_operations=8
'''.encode()
write('pilote.toml',raw)
write('pilote.sig',key(1).sign(envelope('NovaOS/driver-package/v1',raw)))

# Deuxième trousseau entièrement distinct pour le profil de distribution fictif.
for name,n in [('pilote',5),('elf',6),('systeme',7)]:
    write('dist-'+name+'.pub',key(n).public_key().public_bytes_raw())
dist=raw.replace(b'origin="laboratoire"',b'origin="distribution"').replace(b'lab-v1',b'dist-v1')
for isa in ['aarch64','x86_64']:
    old=(OUT/(isa+'.pkg01')).read_bytes()
    n=struct.unpack_from('<I',old)[0]
    pkg=old[:4+n]+key(6).sign(old[4:4+n])+old[4+n+64:]
    new_hash=write('dist-'+isa+'.pkg01',pkg)
    dist=dist.replace(sha256(old).hexdigest().encode(),new_hash.encode())
write('dist-pilote.toml',dist)
write('dist-pilote.sig',key(5).sign(envelope('NovaOS/driver-package/v1',dist)))
dist_record=record.replace(b'origin="laboratoire"',b'origin="distribution"')
write('dist-profil.toml',dist_record)
write('dist-profil.sig',key(7).sign(envelope('NovaOS/driver-resource/v1',dist_record)))
