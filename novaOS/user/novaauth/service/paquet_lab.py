"""Prépare le manifeste public et assemble une signature reçue séparément de l'autorité."""
import argparse
import hashlib
from pathlib import Path
import re
import struct

MACHINES = {"x86_64": 62, "aarch64": 183}
MAGIC = b"NovaOS/ELF/pkg01"


def build(identity, version, arch, files):
    if len(identity) != 16 or not any(identity) or not 0 < version <= 0xffffffff:
        raise ValueError("identité ou version de paquet invalide")
    if not 1 <= len(files) <= 8 or len({name for name, _ in files}) != len(files):
        raise ValueError("membres absents, dupliqués ou trop nombreux")
    manifest = bytearray(MAGIC + identity + struct.pack("<IHH", version, MACHINES[arch], len(files)))
    payload = bytearray()
    for name, path in sorted(files):
        if not re.fullmatch("[A-Za-z0-9][A-Za-z0-9._-]{0,62}", name):
            raise ValueError("nom canonique invalide")
        with path.open("rb") as src:
            content = src.read(4*1024*1024 + 1 - len(payload))
        if len(payload)+len(content)>4*1024*1024:
            raise ValueError("taille de paquet excessive")
        if (len(content)<64 or content[:7]!=b"\x7fELF\x02\x01\x01"
                or struct.unpack_from("<HH",content,16)!=(2,MACHINES[arch])):
            raise ValueError("ELF incompatible")
        manifest.append(len(name))
        manifest.extend(name.encode("ascii"))
        manifest.extend(struct.pack("<I",len(content)) + hashlib.sha256(content).digest())
        payload.extend(content)
    return manifest,payload


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("architecture",choices=MACHINES)
    p.add_argument("identite",type=bytes.fromhex)
    p.add_argument("version",type=int)
    p.add_argument("sortie",type=Path)
    p.add_argument("--signature",type=Path)
    p.add_argument("membres",nargs="+",metavar="NOM=ELF")
    a=p.parse_args()
    entries=[]
    for item in a.membres:
        name,sep,path=item.partition("=")
        if not sep: p.error("membre attendu sous la forme NOM=ELF")
        entries.append((name,Path(path)))
    manifest,payload=build(a.identite,a.version,a.architecture,entries)
    if a.signature:
        sig=a.signature.read_bytes()
        if len(sig)!=64: p.error("signature Ed25519 de 64 octets attendue")
        data=struct.pack("<I",len(manifest))+manifest+sig+payload
    else:
        data=manifest
    with a.sortie.open("xb") as out:
        out.write(data)

if __name__=="__main__":
    main()
