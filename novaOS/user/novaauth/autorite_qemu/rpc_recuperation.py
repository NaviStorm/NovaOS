"""Dispatcher de laboratoire : TLS authentifié, décideur durable et détenteurs séparés.

Les callbacks d'administration et de remise représentent les acteurs indépendants.
Ce module ne reçoit ni clé privée de détenteur, ni clé privée de l'incarnation.
"""
import secrets
import struct
from contextlib import closing

from reprise import Refus, fixed, open_database
from recuperation import (ContextV2, digest, verify, SERVER_BINDING, POSSESSION,
    ADMIN, RESERVE, QUERY, DELIVERY_STATE)
from cles_recuperation import Enrollment

HEADER=struct.Struct(">8sIHH")
MAX=2048

def header(sequence,operation,size):
    if not 0<sequence<2**32 or not 0<operation<2**16 or not 0<=size<=MAX-16:
        raise Refus("encadrement hors borne")
    return HEADER.pack(b"NARPC002",sequence,operation,size)

def decode_header(data):
    if len(data)!=16:
        raise Refus("en-tête tronqué")
    magic,sequence,operation,size=HEADER.unpack(data)
    if magic!=b"NARPC002" or header(sequence,operation,size)!=data:
        raise Refus("encodage non canonique")
    return sequence,operation,size

class Dispatcher:
    def __init__(self,authority,enrollment,server,domain,peer_key,measurement,
                 manifests,ciphertexts,approve,deliver,retrieve):
        self.authority=authority
        self.enrollment=enrollment
        self.server=fixed(server,16);self.domain=fixed(domain,16)
        self.peer_key=fixed(peer_key,32)
        self.measurement=fixed(measurement,32)
        self.manifests=manifests
        self.ciphertexts=ciphertexts
        self.approve_independently=approve
        self.deliver_independently=deliver
        self.retrieve_independently=retrieve
        self.last=0
        self.instance=None
        self.context=None
        self.server_signature=None
        self.admin_signature=None
        self.possession=None
        self.reservations={}

    def check_context(self,op):
        if self.context is None or self.context.op!=op:
            raise Refus("récupération inconnue sur cette route")
        return self.context

    def request(self,sequence,operation,body):
        # Une signature locale BIND ne traverse pas TLS ; les trous de séquence
        # sont admis, jamais un retour en arrière ni la répétition d'une mutation.
        if sequence<=self.last or len(body)<64 or len(body)>MAX-16:
            raise Refus("séquence ou taille refusée")
        self.last=sequence
        if body[:16]!=self.server or body[16:32]!=self.domain:
            raise Refus("substitution de serveur ou domaine")
        instance=fixed(body[32:64],32)
        if self.instance is not None and instance!=self.instance:
            raise Refus("substitution d'instance")
        self.instance=instance
        # Le certificat TLS doit correspondre à la clé enregistrée, même pour
        # une requête ne déclenchant pas de consultation de tête.
        with closing(open_database(self.authority.path)) as db:
            scope=self.authority._scope(db,self.server,self.domain)
            if scope[4]!=self.peer_key:
                raise Refus("pair TLS non enregistré")
        q=body[64:]
        if operation==10 and len(q)==32:
            state,sig=self.authority.authority.observe_peer(self.server,self.domain,q,self.peer_key)
            return state.packet()+sig
        if operation==11 and len(q)==40:
            data,sig=self.enrollment.observe(self.server,self.domain,int.from_bytes(q[:8],"big"),q[8:])
            return data+sig
        if operation==12 and len(q)==40:
            matches=[m for m in self.manifests.values() if m[78:86]+m[270:302]==q
                     and m[14:30]==self.server and m[30:46]==self.domain]
            if len(matches)!=1:
                raise Refus("sauvegarde absente ou ambiguë")
            return matches[0]
        if operation==13 and len(q)==128 and self.context is None:
            manifest=self.manifests.get(q[:32])
            if manifest is None:
                raise Refus("sauvegarde inconnue")
            (old,epoch,generation,head),nonce,until=self.authority.challenge(self.server,self.domain)
            c=ContextV2(secrets.token_bytes(16),self.server,self.domain,old,self.instance,
                epoch,generation,head,q[:32],int.from_bytes(manifest[86:94],"big"),nonce,until,
                q[32:64],q[96:128],self.measurement,q[64:96])
            c.packet()
            self.context=c
            return c.packet()
        if operation==14 and len(q)==504:
            c=self.check_context(q[8:24])
            if q[:376]!=c.packet():
                raise Refus("contexte remplacé après proposition")
            server_signature,pop=q[376:440],q[440:]
            verify(self.peer_key,SERVER_BINDING,c.packet(),server_signature)
            verify(c.possession,POSSESSION,c.packet(),pop)
            admin=self.approve_independently(c,server_signature,pop)
            data,sig=self.authority.approve(c,admin,server_signature,pop)
            self.server_signature,self.admin_signature,self.possession=server_signature,admin,pop
            return data+sig
        if operation==15 and len(q)==129:
            c=self.check_context(q[:16]);role=q[16];delivery=q[17:33];nonce=q[33:65]
            data,sig=self.authority.reserve(c.op,role,delivery,nonce,q[65:])
            self.reservations[role]=(delivery,nonce,data,sig)
            return data+sig
        if operation==16 and len(q)==309:
            c=self.check_context(q[:16]);role=q[16]
            reservation=self.reservations.get(role)
            if reservation is None:
                raise Refus("réservation absente, réconcilier explicitement")
            delivery,nonce,data,sig=reservation
            # Le détenteur vérifie indépendamment l'approbation, le contexte
            # courant et l'admission durable avant toute émission de chiffré.
            envelope=self.deliver_independently(role,c,self.manifests[c.manifest],delivery,nonce,
                self.server_signature,self.admin_signature,self.possession,data,sig,q[17:])
            if len(envelope)!=324:
                raise Refus("remise non canonique")
            return envelope
        if operation==22 and len(q)==113:
            c=self.check_context(q[:16]);role=q[16]
            if role not in (1,2):
                raise Refus("rôle de consultation invalide")
            verify(c.possession,b"NovaAuth/recovery/retrieve/v2\0",
                   c.hash()+bytes([role])+q[17:49],q[49:])
            envelope=self.retrieve_independently(role,c,self.manifests[c.manifest],
                self.server_signature,self.admin_signature,self.possession)
            if len(envelope)!=324:
                raise Refus("issue de remise non canonique")
            return envelope
        if operation==17 and len(q)==113:
            c=self.check_context(q[:16])
            self.authority.acknowledge(c.op,q[16],q[17:49],q[49:])
            return b""
        if operation==18 and len(q)==112:
            c=self.check_context(q[:16])
            data,sig=self.authority.activate(c.op,q[16:48],q[48:])
            return data+sig
        if operation==19 and len(q)==36:
            c=self.check_context(self.context.op) if self.context is not None else None
            if c is None or q[:32]!=c.manifest:
                raise Refus("sauvegarde substituée")
            chunks=self.ciphertexts[c.manifest]
            index=int.from_bytes(q[32:],"big")
            if index>=len(chunks):
                raise Refus("fragment hors limites")
            chunk=chunks[index]
            if not 17<=len(chunk)<=1040:
                raise Refus("fragment incomplet")
            return chunk
        if operation in (20,21) and len(q)==112:
            c=self.check_context(q[:16])
            call=self.authority.outcome if operation==20 else self.authority.observe
            data,sig=call(c.op,q[16:48],q[48:])
            return data+sig
        raise Refus("opération ou taille refusée")
