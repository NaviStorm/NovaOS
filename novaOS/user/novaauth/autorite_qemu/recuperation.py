"""Ordre durable de la récupération 2-sur-2, exclusivement pour QEMU.

Les secrets des détenteurs et du destinataire ne traversent jamais cette API.
Les méthodes publiques reçoivent des signatures typées. Les consultations
d'issue sont distinctes des demandes de mutation.
"""
from dataclasses import dataclass
from hashlib import sha256
import secrets
import struct
import time

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from reprise import Refus, fixed, transaction, open_database
from contextlib import closing

CONTEXT = struct.Struct(">8s16s16s16s32s32sQQ32s32sQ32sQ32s32s32s")
SERVER_BINDING = b"NovaAuth/recovery/instance-binding/v1\0"
POSSESSION = b"NovaAuth/recovery/possession/v1\0"
ADMIN = b"NovaAuth/recovery/approval/v1\0"
RECEIPT = b"NovaAuth/recovery/receipt/v1\0"
RESERVE = b"NovaAuth/recovery/reserve/v1\0"
ACK = b"NovaAuth/recovery/ack/v1\0"
QUERY = b"NovaAuth/recovery/query/v1\0"
ACTIVATE = b"NovaAuth/recovery/activate/v1\0"
REGISTER = b"NovaAuth/recovery/register-backup/v1\0"
ROTATE = b"NovaAuth/recovery/key-epoch/v1\0"
DELIVERY_RECORD = b"NovaAuth/recovery/delivery-record/v1\0"
DELIVERY_STATE = b"NovaAuth/recovery/delivery-state/v1\0"

SCHEMA = """
CREATE TABLE recovery_keys(server BLOB, domain BLOB, epoch INTEGER,
    guardian BLOB NOT NULL, administrator BLOB NOT NULL,
    guardian_sign BLOB NOT NULL, administrator_sign BLOB NOT NULL,
    active INTEGER NOT NULL CHECK(active IN (0,1)),
    PRIMARY KEY(server,domain,epoch));
CREATE TABLE recovery_backups(manifest BLOB PRIMARY KEY, server BLOB NOT NULL,
    domain BLOB NOT NULL, generation INTEGER NOT NULL, head BLOB NOT NULL,
    key_epoch INTEGER NOT NULL, packet BLOB NOT NULL, receipt BLOB NOT NULL);
CREATE TABLE recovery_operations(op BLOB PRIMARY KEY, context BLOB NOT NULL,
    state TEXT NOT NULL, decision BLOB NOT NULL, receipt BLOB NOT NULL,
    authority_boot BLOB NOT NULL, deadline INTEGER NOT NULL);
CREATE TABLE recovery_deliveries(op BLOB, role INTEGER CHECK(role IN (1,2)),
    id BLOB UNIQUE NOT NULL, nonce BLOB NOT NULL, envelope BLOB, ack BLOB,
    PRIMARY KEY(op,role), FOREIGN KEY(op) REFERENCES recovery_operations(op));
CREATE TABLE recovery_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    op BLOB NOT NULL, event TEXT NOT NULL, digest BLOB NOT NULL);
"""


def verify(key, domain, data, signature):
    try:
        Ed25519PublicKey.from_public_bytes(fixed(key, 32)).verify(
            fixed(signature, 64), domain + data)
    except Exception as error:
        raise Refus("signature ou identité refusée") from error


def digest(data):
    return sha256(data).digest()


def initialize(path):
    with closing(open_database(path)) as db:
        db.execute("PRAGMA foreign_keys=ON")
        db.executescript("BEGIN IMMEDIATE;" + SCHEMA)
        db.commit()
    from cles_recuperation import initialize as initialize_receivers
    initialize_receivers(path)


@dataclass(frozen=True)
class Context:
    op: bytes
    server: bytes
    domain: bytes
    old: bytes
    new: bytes
    route_epoch: int
    generation: int
    head: bytes
    manifest: bytes
    key_epoch: int
    challenge: bytes
    until: int
    receiver: bytes
    possession: bytes
    measurement: bytes

    def packet(self):
        for name, size in (("op", 16), ("server", 16), ("domain", 16),
                           ("old", 32), ("new", 32), ("head", 32),
                           ("manifest", 32), ("challenge", 32), ("receiver", 32),
                           ("possession", 32), ("measurement", 32)):
            fixed(getattr(self, name), size)
        if not 0 <= self.route_epoch < 2**63-1:
            raise Refus("époque de route invalide")
        for value in (self.generation, self.key_epoch, self.until):
            if not 0 < value < 2**63:
                raise Refus("compteur ou échéance invalide")
        if self.old == self.new or self.receiver == self.possession:
            raise Refus("incarnation ou clés réutilisées")
        return CONTEXT.pack(b"NARECV01", self.op, self.server, self.domain, self.old, self.new,
                            self.route_epoch, self.generation, self.head, self.manifest,
                            self.key_epoch, self.challenge, self.until, self.receiver,
                            self.possession, self.measurement)

    @classmethod
    def decode(cls, data):
        if len(data) == 376 and data[:8] == b"NARECV02":
            return ContextV2.decode(data)
        if len(data) != CONTEXT.size:
            raise Refus("taille du contexte")
        fields = CONTEXT.unpack(data)
        if fields[0] != b"NARECV01":
            raise Refus("version de récupération")
        context = cls(*fields[1:])
        if context.packet() != data:
            raise Refus("encodage non canonique")
        return context

    def hash(self):
        return digest(self.packet())


CONTEXT_V2 = struct.Struct(">8s16s16s16s32s32sQQ32s32sQ32sQ32s32s32s32s")

@dataclass(frozen=True)
class ContextV2(Context):
    receiver_administrator: bytes

    def packet(self):
        # Conserver tous les contrôles du contexte v1 et ajouter le second rôle.
        super().packet()
        fixed(self.receiver_administrator,32)
        if self.receiver_administrator in (self.receiver,self.possession):
            raise Refus("destinataires de rôle confondus")
        return CONTEXT_V2.pack(
            b"NARECV02",self.op,self.server,self.domain,self.old,self.new,
            self.route_epoch,self.generation,self.head,self.manifest,self.key_epoch,
            self.challenge,self.until,self.receiver,self.receiver_administrator,
            self.possession,self.measurement)

    @classmethod
    def decode(cls,data):
        if len(data) != CONTEXT_V2.size:
            raise Refus("taille du contexte v2")
        f = CONTEXT_V2.unpack(data)
        if f[0] != b"NARECV02":
            raise Refus("version du contexte v2")
        context = cls(*f[1:14],f[15],f[16],f[14])
        if context.packet() != data:
            raise Refus("encodage v2 non canonique")
        return context

    def recipient(self,role):
        if role == 1:
            return self.receiver
        if role == 2:
            return self.receiver_administrator
        raise Refus("rôle inconnu")


def event(db, op, name, data):
    # Identifiants et empreintes uniquement.
    db.execute("INSERT INTO recovery_events(op,event,digest) VALUES(?,?,?)",
               (op, name, digest(data)))


class RecoveryAuthority:
    def __init__(self, authority, measurements, clock=time.monotonic_ns):
        self.authority = authority
        self.path = authority.path
        self.key = authority.key
        self.clock = clock
        self.measurements = frozenset(fixed(x, 32) for x in measurements)
        self.boot = secrets.token_bytes(32)
        self.challenges = {}

    def _scope(self, db, server, domain):
        row = db.execute(
            "SELECT instance,epoch,generation,head,server_key,admin_key "
            "FROM scope WHERE server=? AND domain=?", (server, domain)).fetchone()
        if row is None:
            raise Refus("domaine inconnu")
        return row

    def _keys(self, db, server, domain, epoch):
        row = db.execute(
            "SELECT guardian,administrator,guardian_sign,administrator_sign,active "
            "FROM recovery_keys WHERE server=? AND domain=? AND epoch=?",
            (server, domain, epoch)).fetchone()
        if row is None or row[4] != 1:
            raise Refus("époque de clés absente ou révoquée")
        return row[:4]

    def provision_keys(self, server, domain, epoch, keys):
        # Installation initiale, effectuée sous le contrôle indépendant du laboratoire.
        if not 0 < epoch < 2**63 or len(keys) != 4 or len(set(keys)) != 4:
            raise Refus("jeu de clés invalide")
        for key in keys:
            fixed(key, 32)
        with transaction(self.path) as db:
            scope = self._scope(db, server, domain)
            if scope[4] in keys or scope[5] in keys or self.key.public_key().public_bytes_raw() in keys:
                raise Refus("clés de rôles confondues")
            if db.execute("SELECT 1 FROM recovery_keys WHERE server=? AND domain=?",
                          (server, domain)).fetchone():
                raise Refus("initialisation déjà effectuée")
            db.execute("INSERT INTO recovery_keys VALUES(?,?,?,?,?,?,?,1)",
                       (server, domain, epoch, *keys))

    def register_backup(self, manifest, instance, signature):
        if len(manifest) != 418 or manifest[:14] != b"NABACK01\x00\x20\x00\x01\xff\xff":
            raise Refus("manifeste invalide")
        server, domain = manifest[14:30], manifest[30:46]
        generation, epoch = struct.unpack(">QQ", manifest[78:94])
        head = manifest[270:302]
        with transaction(self.path) as db:
            live, _, gen, current, server_key, _ = self._scope(db, server, domain)
            verify(server_key, REGISTER, instance + manifest, signature)
            if (live, gen, current) != (instance, generation, head):
                raise Refus("capture hors de la tête courante")
            keys = self._keys(db, server, domain, epoch)
            if manifest[94:126] != digest(keys[0]) or manifest[126:158] != digest(keys[1]):
                raise Refus("clés de capture non autorisées")
            if manifest[158:174] != domain or manifest[246:254] != manifest[78:86]:
                raise Refus("tête du manifeste divergente")
            lengths = struct.unpack(">III", manifest[366:378])
            chunks, size = struct.unpack(">II", manifest[378:386])
            if size != 1024 or any(not 0 < n <= 16384 for n in lengths) or chunks != sum((n+1023)//1024 for n in lengths):
                raise Refus("bornes du manifeste")
            m = digest(manifest)
            packet = server + domain + m + manifest[78:94] + head
            receipt = self.key.sign(REGISTER + packet)
            db.execute("INSERT INTO recovery_backups VALUES(?,?,?,?,?,?,?,?)",
                       (m, server, domain, generation, head, epoch, packet, receipt))
            event(db, m[:16], "backup-registered", packet)
            return packet, receipt

    def challenge(self, server, domain):
        with closing(open_database(self.path)) as db:
            scope = self._scope(db, server, domain)
        nonce = secrets.token_bytes(32)
        until = self.clock() + 30_000_000_000
        self.challenges[nonce] = (server, domain, scope[:4], until)
        return scope[:4], nonce, until

    def approve(self, context, admin_signature, server_signature, proof):
        packet = context.packet()
        entry = self.challenges.get(context.challenge)
        expected = (context.server, context.domain,
                    (context.old, context.route_epoch, context.generation, context.head), context.until)
        if entry != expected or self.clock() >= context.until:
            raise Refus("défi absent, expiré ou substitué")
        if context.measurement not in self.measurements:
            raise Refus("mesure du registre non approuvée")
        with transaction(self.path) as db:
            row = self._scope(db, context.server, context.domain)
            if row[:4] != expected[2]:
                raise Refus("tête ou route modifiée")
            verify(row[4], SERVER_BINDING, packet, server_signature)
            verify(row[5], ADMIN, packet, admin_signature)
            verify(context.possession, POSSESSION, packet, proof)
            self._keys(db, context.server, context.domain, context.key_epoch)
            if isinstance(context,ContextV2):
                from cles_recuperation import receiver_keys
                receiver_keys(db,context.server,context.domain,context.key_epoch)
            backup = db.execute("SELECT server,domain,generation,head,key_epoch "
                                "FROM recovery_backups WHERE manifest=?", (context.manifest,)).fetchone()
            if backup != (context.server, context.domain, context.generation, context.head, context.key_epoch):
                raise Refus("sauvegarde étrangère ou obsolète")
            if db.execute("SELECT 1 FROM used WHERE server=? AND domain=? AND instance=?",
                          (context.server, context.domain, context.new)).fetchone():
                raise Refus("incarnation déjà utilisée")
            # Retrait durable avant toute admission de la nouvelle incarnation.
            db.execute("UPDATE recovery_operations SET state='revoked' WHERE state IN ('approved','prepared') "
                       "AND substr(context,25,16)=? AND substr(context,41,16)=?",
                       (context.server, context.domain))
            db.execute("INSERT INTO used VALUES(?,?,?)", (context.server, context.domain, context.new))
            db.execute("UPDATE scope SET instance=?,epoch=epoch+1 WHERE server=? AND domain=?",
                       (context.new, context.server, context.domain))
            decision = context.hash() + struct.pack(">Q", context.route_epoch+1)
            receipt = self.key.sign(RECEIPT + decision)
            deadline = self.clock() + 300_000_000_000
            db.execute("INSERT INTO recovery_operations VALUES(?,?,?,?,?,?,?)",
                       (context.op, packet, "approved", decision, receipt, self.boot, deadline))
            event(db, context.op, "incarnation-approved", decision)
        del self.challenges[context.challenge]
        return decision, receipt

    def _active(self, db, op):
        row = db.execute("SELECT context,state,authority_boot,deadline FROM recovery_operations WHERE op=?", (op,)).fetchone()
        if row is None or row[1] not in ("approved", "prepared") or row[2] != self.boot or self.clock() >= row[3]:
            raise Refus("récupération inactive ou expirée")
        context = Context.decode(row[0])
        scope = self._scope(db, context.server, context.domain)
        if scope[:4] != (context.new, context.route_epoch+1, context.generation, context.head):
            raise Refus("route ou tête devenue obsolète")
        self._keys(db, context.server, context.domain, context.key_epoch)
        if isinstance(context,ContextV2):
            from cles_recuperation import receiver_keys
            receiver_keys(db,context.server,context.domain,context.key_epoch)
        return context

    def observe(self, op, fresh, proof):
        fixed(fresh,32)
        with transaction(self.path) as db:
            context = self._active(db,op)
            verify(context.possession, QUERY, op+fresh, proof)
            packet = context.hash()+fresh
            return packet,self.key.sign(QUERY+packet)

    def reserve(self, op, role, delivery, nonce, proof):
        if role not in (1,2):
            raise Refus("rôle inconnu")
        fixed(delivery,16)
        fixed(nonce,32)
        with transaction(self.path) as db:
            context=self._active(db,op)
            data=context.hash()+bytes([role])+delivery+nonce
            verify(context.possession, RESERVE,data,proof)
            db.execute("INSERT INTO recovery_deliveries(op,role,id,nonce) VALUES(?,?,?,?)",
                       (op,role,delivery,nonce))
            event(db,op,"delivery-reserved",data)
            return data,self.key.sign(RESERVE+data)

    def record_delivery(self, op, role, envelope, holder_signature):
        if role not in (1,2) or len(envelope)!=324:
            raise Refus("remise invalide")
        with transaction(self.path) as db:
            context=self._active(db,op)
            keys=self._keys(db,context.server,context.domain,context.key_epoch)
            verify(keys[role+1], RESERVE,context.hash()+digest(envelope),holder_signature)
            row=db.execute("SELECT id,nonce,envelope FROM recovery_deliveries WHERE op=? AND role=?",
                           (op,role)).fetchone()
            if row is None or row[2] is not None:
                raise Refus("remise absente ou déjà enregistrée")
            if envelope[:10]!=b"NASHAR01"+bytes([1,role]) or envelope[10:42]!=context.hash() or envelope[42:58]!=row[0]:
                raise Refus("substitution de remise")
            verify(keys[role+1], b"", envelope[:260],envelope[260:])
            db.execute("UPDATE recovery_deliveries SET envelope=? WHERE op=? AND role=?",(digest(envelope),op,role))
            event(db,op,"delivery-recorded",digest(envelope))
            record=context.hash()+bytes([role])+row[0]+digest(envelope)
            receipt=self.key.sign(DELIVERY_RECORD+record)
        return record,receipt

    def observe_holder(self, op, role, fresh, signature):
        if role not in (1,2):
            raise Refus("rôle inconnu")
        fixed(fresh,32)
        with transaction(self.path) as db:
            context=self._active(db,op)
            keys=self._keys(db,context.server,context.domain,context.key_epoch)
            verify(keys[role+1],QUERY,op+bytes([role])+fresh,signature)
            packet=context.hash()+fresh
            return packet,self.key.sign(QUERY+packet)

    def delivery_outcome(self, op, role, fresh, signature):
        if role not in (1,2):
            raise Refus("rôle inconnu")
        fixed(fresh,32)
        with transaction(self.path) as db:
            context=self._active(db,op)
            keys=self._keys(db,context.server,context.domain,context.key_epoch)
            verify(keys[role+1],DELIVERY_STATE,op+bytes([role])+fresh,signature)
            row=db.execute("SELECT id,envelope FROM recovery_deliveries WHERE op=? AND role=?",(op,role)).fetchone()
            if row is None:
                raise Refus("réservation inconnue")
            packet=context.hash()+bytes([role])+row[0]+(row[1] or bytes(32))+bytes([row[1] is not None])+fresh
            return packet,self.key.sign(DELIVERY_STATE+packet)

    def acknowledge(self, op, role, envelope_hash, proof):
        if role not in (1,2):
            raise Refus("rôle inconnu")
        with transaction(self.path) as db:
            context=self._active(db,op)
            row=db.execute("SELECT id,envelope,ack FROM recovery_deliveries WHERE op=? AND role=?",(op,role)).fetchone()
            if row is None or row[1]!=fixed(envelope_hash,32):
                raise Refus("remise non enregistrée")
            data=bytes([role])+row[0]+envelope_hash
            verify(context.possession,ACK,data,proof)
            if row[2] is not None and row[2]!=proof:
                raise Refus("acquittement contradictoire")
            if row[2] is None:
                db.execute("UPDATE recovery_deliveries SET ack=? WHERE op=? AND role=?",(proof,op,role))
                event(db,op,"delivery-acknowledged",data)

    def activate(self, op, head, proof):
        with transaction(self.path) as db:
            context=self._active(db,op)
            data=context.hash()+fixed(head,32)
            verify(context.possession,ACTIVATE,data,proof)
            if context.head!=head:
                raise Refus("triplet différent de la tête")
            if db.execute("SELECT count(*) FROM recovery_deliveries WHERE op=? AND ack IS NOT NULL",(op,)).fetchone()[0]!=2:
                raise Refus("les deux remises sont nécessaires")
            receipt=self.key.sign(ACTIVATE+data)
            db.execute("UPDATE recovery_operations SET state='activated',decision=?,receipt=? WHERE op=?",(data,receipt,op))
            event(db,op,"activated",data)
            return data,receipt

    def outcome(self, op, fresh, proof):
        # Une issue historique est consultable même après révocation. Elle ne
        # produit ni activation ni remise. La réponse engage un nonce frais.
        fixed(fresh,32)
        with closing(open_database(self.path)) as db:
            row=db.execute("SELECT context,state,decision,receipt FROM recovery_operations WHERE op=?",(op,)).fetchone()
        if row is None:
            raise Refus("opération inconnue")
        context=Context.decode(row[0])
        verify(context.possession,QUERY,op+fresh,proof)
        state=row[1].encode("ascii")
        data=context.hash()+fresh+bytes([len(state)])+state+row[2]+row[3]
        return data,self.key.sign(QUERY+data)

    def revoke_key_epoch(self, server, domain, epoch, guardian, administrator):
        with transaction(self.path) as db:
            keys=self._keys(db,server,domain,epoch)
            data=server+domain+struct.pack(">Q",epoch)+b"revoke"
            verify(keys[2],ROTATE,data,guardian)
            verify(keys[3],ROTATE,data,administrator)
            db.execute("UPDATE recovery_keys SET active=0 WHERE server=? AND domain=? AND epoch=?",(server,domain,epoch))
            event(db,digest(data)[:16],"key-epoch-revoked",data)

    def rotate_keys(self, server, domain, epoch, new_keys, guardian, administrator):
        if len(new_keys) != 4 or len(set(new_keys)) != 4 or epoch >= 2**63-2:
            raise Refus("nouveau jeu de clés invalide")
        for key in new_keys:
            fixed(key,32)
        with transaction(self.path) as db:
            old=self._keys(db,server,domain,epoch)
            scope=self._scope(db,server,domain)
            if set(new_keys) & {scope[4],scope[5],self.key.public_key().public_bytes_raw()}:
                raise Refus("confusion des rôles")
            data=server+domain+struct.pack(">QQ",epoch,epoch+1)+b"".join(new_keys)
            verify(old[2],ROTATE,data,guardian)
            verify(old[3],ROTATE,data,administrator)
            db.execute("INSERT INTO recovery_keys VALUES(?,?,?,?,?,?,?,1)",
                       (server,domain,epoch+1,*new_keys))
            event(db,digest(data)[:16],"key-epoch-added",data)
            # Le retrait de l'ancienne époque est une opération distincte.
            return data,self.key.sign(ROTATE+data)
