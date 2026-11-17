"""Oracle indépendant RFC 9180, réservé aux tests synthétiques.

Primitives OpenSSL via cryptography. Les opérations de service emploient
novaos-recovery. Aucun appel réseau ni clé du laboratoire n'entre ici.
"""
from hashlib import sha256
import hmac
import struct

from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

def extract(salt,ikm):
    return hmac.digest(salt or bytes(32),ikm,"sha256")

def expand(prk,info,length):
    if length>255*32:
        raise ValueError("HKDF trop long")
    out=b"";previous=b""
    for n in range(1,(length+31)//32+1):
        previous=hmac.digest(prk,previous+info+bytes([n]),"sha256")
        out+=previous
    return out[:length]

def labeled_extract(suite,salt,label,ikm):
    return extract(salt,b"HPKE-v1"+suite+label+ikm)

def labeled_expand(suite,prk,label,info,length):
    return expand(prk,struct.pack(">H",length)+b"HPKE-v1"+suite+label+info,length)

def schedule(private,encapsulated,recipient,info,aead):
    dh=private.exchange(X25519PublicKey.from_public_bytes(encapsulated))
    kem=b"KEM\x00\x20"
    eae=labeled_extract(kem,b"",b"eae_prk",dh)
    shared=labeled_expand(kem,eae,b"shared_secret",encapsulated+recipient,32)
    suite=b"HPKE\x00\x20\x00\x01"+struct.pack(">H",aead)
    context=b"\0"+labeled_extract(suite,b"",b"psk_id_hash",b"")+labeled_extract(suite,b"",b"info_hash",info)
    secret=labeled_extract(suite,shared,b"secret",b"")
    exporter=labeled_expand(suite,secret,b"exp",context,32)
    return suite,context,secret,exporter

def export_share(private,encapsulated,context,role,keyid):
    info=b"NovaAuth/backup/HPKE-info/v1\0"+sha256(context).digest()+bytes([role])+keyid
    suite,_,_,exporter=schedule(private,encapsulated,private.public_key().public_bytes_raw(),info,65535)
    return labeled_expand(suite,exporter,b"sec",b"NovaAuth/backup/share/v1",32)

def capture(keys,head,generation=13,epoch=2):
    """Deux clés publiques seulement sont nécessaires à la capture."""
    public=[k.public_key().public_bytes_raw() for k in keys]
    context=(b"NABACK01\x00\x20\x00\x01\xff\xff"+b"s"*16+b"d"*16+b"b"*32+
             struct.pack(">QQ",generation,epoch)+b"".join(sha256(p).digest() for p in public)+head)
    assert len(context)==302
    ephemeral=[X25519PrivateKey.generate(),X25519PrivateKey.generate()]
    enc=[k.public_key().public_bytes_raw() for k in ephemeral]
    shares=[]
    for i in range(2):
        # L'oracle destinataire retrouve la même part par décapsulation.
        shares.append(export_share(keys[i],enc[i],context,i+1,sha256(public[i]).digest()))
    objects=[b"users123",b"groups12",b"creds123"]
    d=context+b"".join(enc)+struct.pack(">IIIII",8,8,8,3,1024)
    data_key=expand(extract(sha256(d).digest(),b"".join(shares)),b"NovaAuth/backup/data-key/v1",32)
    cipher=[]
    for i,obj in enumerate(objects):
        aad=sha256(d).digest()+bytes([i+1])+struct.pack(">III",i,len(obj),3)
        cipher.append(AESGCM(data_key).encrypt(i.to_bytes(12,"big"),obj,aad))
    manifest=d+sha256(b"".join(cipher)).digest()
    return manifest,cipher,objects

def open_delivery(private,envelope):
    enc=envelope[58:90]
    suite,context,secret,_=schedule(private,enc,private.public_key().public_bytes_raw(),
                                   b"NovaAuth/recovery/delivery/v1",2)
    key=labeled_expand(suite,secret,b"key",context,32)
    nonce=labeled_expand(suite,secret,b"base_nonce",context,12)
    return AESGCM(key).decrypt(nonce,envelope[90:260],envelope[:58])


def seal_request(recipient, context, role, delivery, nonce, possession):
    """Oracle de tests. La vraie incarnation utilise le composant Rust invité."""
    ephemeral=X25519PrivateKey.generate()
    enc=ephemeral.public_key().public_bytes_raw()
    dh=ephemeral.exchange(X25519PublicKey.from_public_bytes(recipient))
    kem=b"KEM\x00\x20"
    shared=labeled_expand(kem,labeled_extract(kem,b"",b"eae_prk",dh),b"shared_secret",enc+recipient,32)
    suite=b"HPKE\x00\x20\x00\x01\x00\x02"
    info=b"NovaAuth/recovery/request/v1"
    schedule_context=b"\0"+labeled_extract(suite,b"",b"psk_id_hash",b"")+labeled_extract(suite,b"",b"info_hash",info)
    secret=labeled_extract(suite,shared,b"secret",b"")
    key=labeled_expand(suite,secret,b"key",schedule_context,32)
    base_nonce=labeled_expand(suite,secret,b"base_nonce",schedule_context,12)
    header=b"NASHRQ01"+bytes([1,role])+context.hash()+delivery
    clear=bytes([1,role])+context.hash()+context.manifest+struct.pack(">Q",context.key_epoch)+delivery+nonce
    encrypted=AESGCM(key).encrypt(base_nonce,clear,header)
    packet=header+enc+encrypted
    return packet+possession.sign(packet)
