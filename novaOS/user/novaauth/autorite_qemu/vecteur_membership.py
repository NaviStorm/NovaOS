"""Vecteur synthétique public : aucun secret de laboratoire n'est lu ou écrit."""
from pathlib import Path
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from reprise import Published
import membership_retirement as m

def vectors():
    key=Ed25519PrivateKey.from_private_bytes(bytes([87])*32)
    state=Published(b'S'*16,b'D'*16,b'I'*32,0,6,b'H'*32)
    source=m.Source(b'A'*16,b'G'*16,3);op=b'M'*16
    reserve=m.Memberships.reservation(state,op,b'N'*32,b'P'*16,1,source,((1,b'V'*32),(2,b'R'*32),(4,b'E'*32)))
    receipt=(b'NAMBRE01'+state.server+state.domain+state.instance+op+source.packet()+m.number(7)+state.head+b'N'*32+m.number(19)+bytes([2,7,7])+bytes(5)+b'F'*32)
    assert len(receipt)==248
    queries=[reserve,state.packet()+op,state.packet()+op+bytes([1])+b'V'*32,
             m.Memberships.admission(state,source,4,b'E'*32,b'J'*16,2,b'L'*32),
             state.packet()+b'J'*16+bytes([4])+b'E'*32+bytes([1])+b'C'*32,
             state.packet()+op+b'F'*32,state.packet()+b'F'*32]
    out=[key.public_key().public_bytes_raw(),receipt+key.sign(m.RECEIPT+receipt)]
    for prefix,q in zip((m.RESERVE,m.PUBLISH,m.EFFECT,m.ADMIT,m.FINISH,m.QUERY,m.PENDING),queries):out.extend((q,key.sign(prefix+q)))
    query=state.packet()+b'J'*16+b'F'*32
    operation=b'NAMBOU01'+query+queries[3]+m.number(12)+bytes([1])+b'C'*32+m.number(19)
    out.extend((query,operation+key.sign(m.QUERY+operation)))
    return ''.join(b.hex()+'\n' for b in out)
if __name__=='__main__':
    import argparse
    p=argparse.ArgumentParser();p.add_argument('output',type=Path);a=p.parse_args();a.output.write_text(vectors())
