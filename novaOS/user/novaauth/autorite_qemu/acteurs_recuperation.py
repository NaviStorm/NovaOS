"""Acteurs privés de banc, dans des processus distincts, avec secrets non exportés.

Ce raccord local de laboratoire ne remplace pas les canaux de production.
Les Pipes hôtes sont privés aux processus créés ; seuls les chiffrés de parts
peuvent en sortir. Le parent ne reçoit jamais les secrets des détenteurs.
"""
from pathlib import Path
import multiprocessing
import os
import subprocess
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from recuperation import (Refus,verify,ADMIN,SERVER_BINDING,POSSESSION,QUERY,
                         DELIVERY_STATE,digest)
from cles_recuperation import ENROLL

def public(key):
    return key.public_key().public_bytes_raw()

def secret_file(path,key):
    with path.open("xb") as f:
        os.chmod(path,0o600)
        f.write(key.private_bytes_raw())
        f.flush()
        os.fsync(f.fileno())

def holder_worker(port,role,root,executable):
    from detenteur import initialize
    from detenteur_v2 import HolderV2,pin_receiver
    root=Path(root)
    root.mkdir(mode=0o700,parents=True,exist_ok=False)
    backup,request=X25519PrivateKey.generate(),X25519PrivateKey.generate()
    signing=Ed25519PrivateKey.generate()
    for name,key in (("backup",backup),("request",request),("signing",signing)):
        secret_file(root/name,key)
    initialize(root/"remises.sqlite",role,public(backup),public(signing))
    pin_receiver(root/"remises.sqlite",public(request))
    port.send(("public",public(backup),public(request),public(signing)))
    holder=None
    def authority(method,*args):
        port.send(("authority",method,args))
        response=port.recv()
        if response[0]!="ok":
            raise Refus(response[1])
        return response[1]
    def current(op,fresh):
        return authority("observe_holder",op,role,fresh,
                         signing.sign(QUERY+op+bytes([role])+fresh))
    def recorded(op,which,fresh):
        if which!=role:
            raise Refus("rôle substitué")
        return authority("delivery_outcome",op,role,fresh,
                         signing.sign(DELIVERY_STATE+op+bytes([role])+fresh))
    while True:
        command,args=port.recv()
        if command=="stop":
            return
        try:
            if command=="configure":
                decision,server,admin,measurements=args
                holder=HolderV2(root/"remises.sqlite",role,public(backup),public(signing),
                    signing,root/"backup",root/"signing",executable,decision,server,admin,
                    measurements,request_key_path=root/"request",request_public=public(request))
                result=None
            elif command=="enroll":
                packet,envelope=args
                # Un détenteur ne signe que le paquet contenant ses trois clés.
                i=role-1
                if len(packet)!=272 or packet[48+32*i:80+32*i]!=public(backup) or \
                   packet[112+32*i:144+32*i]!=public(request) or \
                   packet[176+32*i:208+32*i]!=public(signing):
                    raise Refus("enregistrement substitué")
                proof=subprocess.check_output([str(executable),"open-key-proof",str(root/"request")],
                                              input=envelope,timeout=10)
                result=(signing.sign(ENROLL+packet),proof)
            elif command=="deliver" and holder is not None:
                c,m,delivery,nonce,server,admin,pop,reservation,sig,request=args
                result=holder.deliver(c,m,delivery,nonce,server,admin,pop,current,
                    reservation,sig,request,lambda *a:authority("record_delivery",*a))
            elif command=="outcome" and holder is not None:
                c,m,server,admin,pop=args
                result=holder.outcome(c,m,server,admin,pop,current,recorded,retrieve=True)
            else:
                raise Refus("opération de détenteur inconnue")
            port.send(("ok",result))
        except Exception as error:
            port.send(("error",type(error).__name__+": "+str(error)))

def administrator_worker(port,root):
    root=Path(root);root.mkdir(mode=0o700,parents=True,exist_ok=False)
    key=Ed25519PrivateKey.generate();secret_file(root/"approbation",key)
    port.send(("public",public(key)))
    bounds=None
    approved=None
    while True:
        command,args=port.recv()
        if command=="stop":
            return
        try:
            if command=="configure":
                bounds=args
                result=None
            elif command=="mandate" and bounds is not None and approved is not None:
                # Action du contrôleur administratif hors invité, jamais du RPC.
                # Nouvelle autorisation explicite, une seule consommation.
                bounds=args;approved=None;result=None
            elif command=="enroll" and bounds is not None:
                packet,=args
                server,domain,server_key,measurement,manifest,generation,head,keys=bounds
                if len(packet)!=272 or packet[8:40]!=server+domain or packet[48:240]!=b"".join(keys):
                    raise Refus("clés d'enrôlement non approuvées")
                result=key.sign(ENROLL+packet)
            elif command=="exec-route" and bounds is not None:
                # Contrôle administratif hors invité, jamais accessible par TLS.
                from admission_exec import ENROLL as EXEC_ENROLL, WITHDRAW, QUERY
                kind,server,domain,instance,fresh=args
                if (server,domain)!=bounds[:2] or len(instance)!=32 or not any(instance):
                    raise Refus("route EXEC hors mandat")
                prefix={"enroll":EXEC_ENROLL,"withdraw":WITHDRAW,"query":QUERY}.get(kind)
                if prefix is None or (kind=="query" and (len(fresh)!=32 or not any(fresh))) or (kind!="query" and fresh):
                    raise Refus("action EXEC invalide")
                packet=server+domain+instance+fresh
                signature=key.sign(prefix+packet)
                with (root/"exec-route.log").open("ab") as f:
                    f.write(prefix+packet+signature);f.flush();os.fsync(f.fileno())
                result=signature
            elif command=="approve" and bounds is not None:
                c,server_signature,pop=args
                server,domain,server_key,measurement,manifest,generation,head,keys=bounds
                if (c.server,c.domain,c.measurement,c.manifest,c.generation,c.head)!= \
                   (server,domain,measurement,manifest,generation,head) or approved is not None:
                    raise Refus("approbation hors mandat unique")
                verify(server_key,SERVER_BINDING,c.packet(),server_signature)
                verify(c.possession,POSSESSION,c.packet(),pop)
                # La décision indépendante est durable avant de rendre sa signature.
                signature=key.sign(ADMIN+c.packet())
                with (root/("approbation-"+c.op.hex()+".bin")).open("xb") as f:
                    f.write(c.packet()+signature);f.flush();os.fsync(f.fileno())
                fd=os.open(root,os.O_RDONLY)
                try:os.fsync(fd)
                finally:os.close(fd)
                approved=c.hash()
                result=signature
            else:
                raise Refus("opération administrative inconnue")
            port.send(("ok",result))
        except Exception as error:
            port.send(("error",type(error).__name__+": "+str(error)))

class Actor:
    def __init__(self,target,*args):
        ctx=multiprocessing.get_context("spawn")
        self.port,child=ctx.Pipe()
        self.process=ctx.Process(target=target,args=(child,*args))
        self.process.start();child.close()
        first=self.port.recv()
        if first[0]!="public":
            raise Refus("acteur non initialisé")
        self.keys=first[1:]
    def call(self,command,*args,authority=None,lose_record_reply=False):
        self.port.send((command,args))
        while True:
            if not self.port.poll(12):
                raise Refus("délai acteur")
            response=self.port.recv()
            if response[0]=="authority":
                try:
                    if authority is None or response[1] not in ("observe_holder","record_delivery","delivery_outcome"):
                        raise Refus("route d'autorité refusée")
                    answer=getattr(authority,response[1])(*response[2])
                    if lose_record_reply and response[1]=="record_delivery":
                        lose_record_reply=False
                        raise ConnectionError("reçu durable perdu après admission")
                    self.port.send(("ok",answer))
                except Exception as error:
                    self.port.send(("error",type(error).__name__+": "+str(error)))
            elif response[0]=="ok":
                return response[1]
            else:
                raise Refus(response[1])
    def close(self):
        if self.process.is_alive():
            self.port.send(("stop",()))
            self.process.join(5)
        if self.process.is_alive():
            self.process.terminate();self.process.join(5)
        self.port.close()
