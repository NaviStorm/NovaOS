"""Remises v2 chiffrées et admission durable avant émission."""
from contextlib import closing
import secrets
import subprocess
from pathlib import Path

from detenteur import Holder, connect
from recuperation import ContextV2, Refus, verify, digest, RESERVE, DELIVERY_RECORD, DELIVERY_STATE
from reprise import fixed

def pin_receiver(path, public):
    with closing(connect(path)) as db:
        db.execute("BEGIN IMMEDIATE")
        db.execute("CREATE TABLE holder_receiver(singleton INTEGER PRIMARY KEY CHECK(singleton=1), public BLOB NOT NULL)")
        db.execute("INSERT INTO holder_receiver VALUES(1,?)",(fixed(public,32),))
        db.commit()

class HolderV2(Holder):
    def __init__(self,*args,request_key_path,request_public,**kwargs):
        super().__init__(*args,**kwargs)
        self.request_key_path=Path(request_key_path)
        with closing(connect(self.path)) as db:
            if db.execute("SELECT public FROM holder_receiver WHERE singleton=1").fetchone() != (fixed(request_public,32),):
                raise Refus("clé de réception du détenteur divergente")

    def _seal(self,c,manifest,delivery,nonce,request):
        if not isinstance(c,ContextV2) or request is None or len(request)!=292:
            raise Refus("requête chiffrée v2 requise")
        return subprocess.run(
            [str(self.executable),"deliver-v2",str(self.role),str(self.key_path),
             str(self.signing_path),str(self.request_key_path)],
            input=manifest+c.packet()+delivery+nonce+request,
            stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=False,timeout=10)

    def deliver(self,c,manifest,delivery,nonce,server_signature,admin_signature,possession,
                current,reservation,reservation_signature,request,record,cut=None):
        if not isinstance(c,ContextV2):
            raise Refus("contexte v2 requis")
        if request is None or len(request)!=292:
            raise Refus("requête canonique de 292 octets requise")
        checked=subprocess.run(
            [str(self.executable),"check-request-v2",str(self.role),str(self.request_key_path)],
            input=c.packet()+fixed(delivery,16)+fixed(nonce,32)+request,
            stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=False,timeout=10)
        if checked.returncode or checked.stdout:
            raise Refus("authentification de la requête refusée")
        envelope,proof=super().deliver(c,manifest,delivery,nonce,server_signature,
            admin_signature,possession,current,reservation,reservation_signature,
            cut=cut,request=request)
        expected=c.hash()+bytes([self.role])+delivery+digest(envelope)
        try:
            decision,signature=record(c.op,self.role,envelope,proof)
        except Exception as error:
            raise Refus("UNCERTAIN admission de la remise, consulter son issue") from error
        if decision!=expected:
            raise Refus("décision de remise divergente")
        verify(self.authority,DELIVERY_RECORD,decision,signature)
        if cut=="admitted":
            raise Refus("UNCERTAIN après admission durable")
        return envelope,proof

    def outcome(self,c,manifest,server_signature,admin_signature,possession,current,
                recorded,retrieve=False):
        # La consultation ne refait jamais la mutation record_delivery.
        result=super().outcome(c,manifest,server_signature,admin_signature,
                               possession,current,retrieve=retrieve)
        if not retrieve:
            return result
        envelope,proof=result
        fresh=secrets.token_bytes(32)
        data,signature=recorded(c.op,self.role,fresh)
        expected=c.hash()+bytes([self.role])+envelope[42:58]+digest(envelope)+b"\x01"+fresh
        if data!=expected:
            raise Refus("remise non durablement admise")
        verify(self.authority,DELIVERY_STATE,data,signature)
        return result
