"""Produit uniquement le paquet du profil de laboratoire sélectionné. Les trois clés de test sont publiques."""
import hashlib
import struct
import sys
from pathlib import Path
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

def digest(data):
    return hashlib.sha256(data).digest()

def signature(seed, domain, data):
    envelope = domain + b"\0" + struct.pack("<I", len(data)) + data if domain else data
    return Ed25519PrivateKey.from_private_bytes(bytes([seed])*32).sign(envelope)

def build(elf, out, isa="aarch64"):
    machine, port, profile = {
        "aarch64": (183, "arm", b"QEMU virt-10.0 PCI 00:07.0 SID56; IRQ38; 2x16; 22 pages; reset 3E; broker only\n"),
        "x86_64": (62, "q35", b"QEMU Q35 PCI 00:07.0 SID56; VT-d; INTx GSI23; 2x16; 22 pages; broker only\n"),
    }[isa]
    if not 64 <= len(elf) <= 4*1024**2 or elf[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<HH", elf, 16) != (2, machine):
        raise ValueError(f"ELF64 {isa} ET_EXEC attendu")
    member = f"virtio-net-{port}".encode("ascii")
    header = (b"NovaOS/ELF/pkg01" + bytes([68])*16 + struct.pack("<IHHB", 1, machine, 1, len(member))
              + member + struct.pack("<I", len(elf)) + digest(elf))
    package = struct.pack("<I", len(header)) + header + signature(2, b"", header) + elf
    record = ('origin="laboratoire"\npolicy_revision=1\nsecurity_epoch=3\n'
              f'id="org.novaos.lab.courtier-{port}"\nkind="system-profile"\nversion=[1,0,0]\n'
              f'sha256="{digest(profile).hex()}"\ndependencies=[]\n').encode()
    manifest = (Path(__file__).with_name("pilote.toml.in").read_text()
                .replace("@ISA@", isa).replace("@PORT@", port)
                .replace("@PAQUET@", digest(package).hex()).replace("@ELF@", digest(elf).hex())
                .replace("@PROFIL@", digest(profile).hex()).encode())
    for name,data in {"pilote.pkg01":package,"pilote.toml":manifest,
                      "pilote.sig":signature(1,b"NovaOS/driver-package/v1",manifest),
                      "profil.bin":profile,"profil.toml":record,
                      "profil.sig":signature(3,b"NovaOS/driver-resource/v1",record)}.items():
        (out/name).write_bytes(data)

if __name__ == "__main__":
    build(Path(sys.argv[1]).read_bytes(), Path(sys.argv[2]), *sys.argv[3:])
