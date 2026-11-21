"""Dépôt de chiffrés hors serveur. Aucun secret ou triplet clair reçu."""
from contextlib import closing
import hashlib
import os
import struct
from reprise import Refus,open_database,fixed
from recuperation import digest,REGISTER,verify

class CaptureStore:
    def __init__(self,root,authority,peer,scope):
        self.root=root
        root.mkdir(parents=True,exist_ok=False)
        self.authority=authority;self.peer=peer;self.scope=scope
    @staticmethod
    def durable(path,data):
        with path.open("xb") as f:
            f.write(data);f.flush();os.fsync(f.fileno())
        fd=os.open(path.parent,os.O_RDONLY)
        try:os.fsync(fd)
        finally:os.close(fd)
    def request(self,operation,body):
        if len(body)<64 or body[:32]!=b"".join(self.scope):raise Refus("portée capture")
        instance=fixed(body[32:64],32);q=body[64:]
        with closing(open_database(self.authority.path)) as db:
            live,_,gen,head,peer,_=self.authority._scope(db,*self.scope)
            if self.peer!=peer:raise Refus("pair non enregistré")
            if operation!=34 and live!=instance:raise Refus("capture incarnation retirée")
            if operation==34 and len(q)==64:
                row=db.execute("SELECT server,domain,packet,receipt FROM recovery_backups WHERE manifest=?",(q[:32],)).fetchone()
                if row is None or row[:2]!=self.scope:raise Refus("issue de capture absente")
                packet,receipt=row[2:]
                proof=self.authority.key.sign(b"NovaAuth/recovery/backup-query/v1\0"+q[32:]+packet+receipt)
                return packet+receipt+proof
        if operation==33 and 53<=len(q)<=1076:
            backup=fixed(q[:32],32);index=int.from_bytes(q[32:36],"big")
            if index>=48:raise Refus("trop de fragments")
            folder=self.root/(instance.hex()+"-"+backup.hex())
            if not folder.exists() and len(list(self.root.iterdir()))>=4:raise Refus("quota captures")
            folder.mkdir(exist_ok=True)
            # Aucun remplacement ni répétition implicite après réponse perdue.
            path=folder/f"{index:02d}.chiffre"
            if path.exists():raise Refus("fragment déjà déposé ; état à réconcilier")
            if index and not (folder/f"{index-1:02d}.chiffre").exists():raise Refus("fragment désordonné")
            self.durable(path,q[36:]);return b""
        if operation==32 and len(q)==482:
            manifest,signature=q[:418],q[418:]
            if manifest[14:46]!=b"".join(self.scope) or manifest[78:86]!=gen.to_bytes(8,"big") or manifest[270:302]!=head:
                raise Refus("capture hors tête durable")
            lengths=struct.unpack(">III",manifest[366:378]);count=int.from_bytes(manifest[378:382],"big")
            if any(not 0<n<=16384 for n in lengths) or count!=sum((n+1023)//1024 for n in lengths):raise Refus("bornes capture")
            folder=self.root/(instance.hex()+"-"+manifest[46:78].hex())
            chunks=[]
            for length in lengths:
                while length:
                    n=min(length,1024);path=folder/f"{len(chunks):02d}.chiffre"
                    if not path.is_file():raise Refus("capture incomplète")
                    chunk=path.read_bytes()
                    if len(chunk)!=n+16:raise Refus("taille fragment")
                    chunks.append(chunk);length-=n
            if len(list(folder.glob("*.chiffre")))!=count or hashlib.sha256(b"".join(chunks)).digest()!=manifest[386:]:
                raise Refus("empreinte capture")
            verify(peer,REGISTER,instance+manifest,signature)
            # Le chiffré est durable avant l'enregistrement de sa tête.
            self.durable(folder/"manifeste.bin",manifest)
            packet,receipt=self.authority.register_backup(manifest,instance,signature)
            self.durable(folder/"recu.bin",packet+receipt)
            return packet+receipt
        raise Refus("opération capture ou longueur invalide")
