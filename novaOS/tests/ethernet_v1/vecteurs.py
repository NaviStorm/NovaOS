"""Corpus indépendant du codec Rust. Entiers little-endian, aucun padding natif."""
import argparse
from pathlib import Path
import struct

def message(op, reply, n, body=b"", req=7):
    return struct.pack("<8HQQ",1,0,op,int(reply),32+len(body),n,0,0,9,req)+body

def corpus():
    desc=struct.pack("<HHIHH",0,60,1,0,0)
    rec=struct.pack("<HHQ",0,0,7)
    link=struct.pack("<BB6xQ",2,2,1_000_000_000)
    requests=[bytes(16),bytes([15])+bytes(7),desc,struct.pack("<Q",7),b"",
              struct.pack("<HHIHH",0,0,1,0,0),bytes(8),b"",b"",
              bytes.fromhex("333300000001"),bytes.fromhex("333300000001"),b"",bytes(8),link]
    replies=[struct.pack("<QQ",1,1),bytes.fromhex("0f080101dc0502000000000100000000"),rec,rec,
             desc,b"",struct.pack("<QB7x",3,4),link,struct.pack("<8Q",1,2,3,4,5,6,7,8),
             b"",b"",b"",b"",b""]
    records=[]
    for op,(request,reply) in enumerate(zip(requests,replies),1):
        records.extend([(0,message(op,False,int(op in (3,6)),request)),(0,message(op,True,int(op in (3,4,5)),reply))])
    many=b"".join(struct.pack("<HHIHH",i,1514,123+i,0,0) for i in range(8))
    records.append((0,message(3,False,8,many)))
    records.append((0,message(5,True,0,b"")))
    records.append((2,message(3,3,0,bytes([6])+bytes(7))))
    records.extend((1,bytes(frame)) for frame in (
        b"",message(3,False,1,desc)[:-1],message(3,False,2,desc*2),
        message(3,False,1,bytes(12)),message(99,False,0),message(4,True,1,rec[:2]+bytes([99,0])+rec[4:]),
        bytes([2])+message(8,False,0)[1:],message(8,False,0)[:14]+bytes([1,0])+message(8,False,0)[16:]
    ))
    return b"ETH1"+b"".join(struct.pack("<BH",kind,len(data))+data for kind,data in records)

if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--write",action="store_true")
    args=parser.parse_args()
    path=Path(__file__).with_name("vecteurs.bin")
    data=corpus()
    if args.write: path.write_bytes(data)
    else: assert path.read_bytes()==data
