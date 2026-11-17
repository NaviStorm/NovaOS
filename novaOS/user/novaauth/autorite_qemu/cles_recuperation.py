"""Enregistrement authentifié des deux clés de réception de laboratoire."""
from contextlib import closing
from hashlib import sha256
import hmac
import secrets
import struct
import subprocess
import threading
import time

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from reprise import Refus, fixed, transaction, open_database

ENROLL = b"NovaAuth/recovery/receiver-keys/v1\0"
OBSERVE = b"NovaAuth/recovery/receiver-keys-current/v1\0"
POP = b"NovaAuth/recovery/receiver-key-pop-binding/v1\0"
RECORD = struct.Struct(">8s16s16sQ32s32s32s32s32s32s32s")

SCHEMA = """
CREATE TABLE IF NOT EXISTS recovery_receivers(
    server BLOB, domain BLOB, epoch INTEGER, role INTEGER CHECK(role IN (1,2)),
    version INTEGER NOT NULL CHECK(version=1),
    suite INTEGER NOT NULL CHECK(suite=32),
    public BLOB NOT NULL CHECK(length(public)=32),
    key_id BLOB NOT NULL CHECK(length(key_id)=32),
    enrollment BLOB NOT NULL CHECK(length(enrollment)=32),
    PRIMARY KEY(server,domain,epoch,role));
CREATE TABLE IF NOT EXISTS recovery_key_enrollments(
    id BLOB PRIMARY KEY CHECK(length(id)=32),
    packet BLOB UNIQUE NOT NULL CHECK(length(packet)=272),
    guardian BLOB NOT NULL, administrator BLOB NOT NULL,
    mandate BLOB NOT NULL, receipt BLOB NOT NULL);
"""

def initialize(path):
    with closing(open_database(path)) as db:
        db.executescript("BEGIN IMMEDIATE;" + SCHEMA + "COMMIT;")

def verify(key, data, signature):
    try:
        Ed25519PublicKey.from_public_bytes(fixed(key,32)).verify(fixed(signature,64),data)
    except Exception as error:
        raise Refus("preuve de clés refusée") from error

def receiver_keys(db, server, domain, epoch):
    rows = db.execute(
        "SELECT role,public,key_id,enrollment,version,suite FROM recovery_receivers "
        "WHERE server=? AND domain=? AND epoch=? ORDER BY role",
        (server,domain,epoch)).fetchall()
    if len(rows) != 2 or [r[0] for r in rows] != [1,2]:
        raise Refus("deux clés de réception enregistrées requises")
    if rows[0][3] != rows[1][3] or rows[0][1] == rows[1][1]:
        raise Refus("jeu de réception divergent")
    for row in rows:
        if row[2] != sha256(row[1]).digest() or row[4:] != (1,32):
            raise Refus("clé de réception altérée")
    records = db.execute(
        "SELECT packet,guardian,administrator,mandate,receipt FROM recovery_key_enrollments "
        "WHERE substr(packet,9,16)=? AND substr(packet,25,16)=? AND substr(packet,41,8)=?",
        (server,domain,epoch.to_bytes(8,"big"))).fetchall()
    if len(records) != 1:
        raise Refus("acte de clés absent ou ambigu")
    packet,guardian,administrator,mandate,receipt = records[0]
    if len(packet) != RECORD.size or sha256(packet).digest() != rows[0][3]:
        raise Refus("acte de clés altéré")
    keys = RECORD.unpack(packet)[4:10]
    pinned = db.execute("SELECT guardian,administrator,guardian_sign,administrator_sign,active "
                        "FROM recovery_keys WHERE server=? AND domain=? AND epoch=?",
                        (server,domain,epoch)).fetchone()
    admin = db.execute("SELECT admin_key FROM scope WHERE server=? AND domain=?",
                       (server,domain)).fetchone()
    authority = db.execute("SELECT public_key FROM authority WHERE singleton=1").fetchone()
    if pinned != (keys[0],keys[1],keys[4],keys[5],1) or keys[2:4] != tuple(r[1] for r in rows):
        raise Refus("clés enregistrées divergentes")
    if admin is None or authority is None:
        raise Refus("autorité de clés absente")
    verify(keys[4],ENROLL+packet,guardian)
    verify(keys[5],ENROLL+packet,administrator)
    verify(admin[0],ENROLL+packet,mandate)
    verify(authority[0],ENROLL+packet,receipt)
    return tuple(row[1] for row in rows)

class Enrollment:
    def __init__(self, authority, executable, clock=time.monotonic_ns):
        self.authority = authority
        self.executable = str(executable)
        self.clock = clock
        self.pending = {}
        self.lock = threading.Lock()

    def challenge(self, server, domain, epoch, recipients):
        if len(recipients) != 2:
            raise Refus("deux destinataires requis")
        with closing(open_database(self.authority.path)) as db:
            keys = self.authority._keys(db,server,domain,epoch)
            scope = self.authority._scope(db,server,domain)
        all_keys = (keys[0],keys[1],*recipients,keys[2],keys[3])
        for key in all_keys:
            fixed(key,32)
        excluded = {scope[4],scope[5],self.authority.key.public_key().public_bytes_raw()}
        if len(set(all_keys)) != 6 or set(all_keys) & excluded:
            raise Refus("clés de rôles confondues")
        nonce = secrets.token_bytes(32)
        packet = RECORD.pack(b"NARKEY01",fixed(server,16),fixed(domain,16),epoch,*all_keys,nonce)
        digest = sha256(packet).digest()
        envelopes, expected = [], []
        for role, recipient in enumerate(recipients,1):
            binding = sha256(POP+digest+bytes([role])).digest()
            secret = secrets.token_bytes(32)
            response = subprocess.run(
                [self.executable,"seal-key-proof"],input=recipient+binding+secret,
                stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=False,timeout=10)
            if response.returncode or len(response.stdout) != 80:
                raise Refus("échec du défi de possession")
            envelopes.append(binding+response.stdout)
            expected.append(sha256(binding+secret).digest())
        with self.lock:
            now = self.clock()
            self.pending = {n:v for n,v in self.pending.items() if v[2] > now}
            if len(self.pending) >= 16:
                raise Refus("LIMIT défis de clés")
            self.pending[nonce] = (packet,tuple(expected),now+30_000_000_000)
        return packet,tuple(envelopes)

    def register(self, packet, guardian, administrator, mandate, proofs):
        if len(packet) != RECORD.size:
            raise Refus("taille de l'enregistrement")
        fields = RECORD.unpack(packet)
        magic,server,domain,epoch = fields[:4]
        keys = fields[4:10]
        nonce = fields[10]
        if magic != b"NARKEY01" or len(proofs) != 2:
            raise Refus("format de l'enregistrement")
        with self.lock:
            entry = self.pending.get(nonce)
            if entry is None or entry[0] != packet or self.clock() >= entry[2]:
                raise Refus("défi de clés absent, substitué ou expiré")
            if not all(hmac.compare_digest(a,b) for a,b in zip(entry[1],proofs)):
                raise Refus("possession de clé non vérifiée")
            with transaction(self.authority.path) as db:
                existing = self.authority._keys(db,server,domain,epoch)
                scope = self.authority._scope(db,server,domain)
                if existing != (keys[0],keys[1],keys[4],keys[5]):
                    raise Refus("jeu de clés modifié")
                verify(existing[2],ENROLL+packet,guardian)
                verify(existing[3],ENROLL+packet,administrator)
                verify(scope[5],ENROLL+packet,mandate)
                if db.execute(
                    "SELECT 1 FROM recovery_receivers WHERE server=? AND domain=? AND epoch=?",
                    (server,domain,epoch)).fetchone():
                    raise Refus("clés déjà enregistrées, consulter le reçu")
                receipt = self.authority.key.sign(ENROLL+packet)
                digest = sha256(packet).digest()
                db.execute("INSERT INTO recovery_key_enrollments VALUES(?,?,?,?,?,?)",
                           (nonce,packet,guardian,administrator,mandate,receipt))
                for role,public in enumerate(keys[2:4],1):
                    db.execute("INSERT INTO recovery_receivers VALUES(?,?,?,?,1,32,?,?,?)",
                               (server,domain,epoch,role,public,sha256(public).digest(),digest))
            del self.pending[nonce]
        return packet,receipt

    def observe(self, server, domain, epoch, fresh):
        fixed(fresh,32)
        with closing(open_database(self.authority.path)) as db:
            self.authority._keys(db,server,domain,epoch)
            keys = receiver_keys(db,server,domain,epoch)
            rows = db.execute(
                "SELECT packet,receipt FROM recovery_key_enrollments "
                "WHERE substr(packet,9,16)=? AND substr(packet,25,16)=? AND substr(packet,41,8)=?",
                (server,domain,epoch.to_bytes(8,"big"))).fetchall()
            if len(rows) != 1:
                raise Refus("reçu de clés incohérent")
            packet,receipt = rows[0]
        observed = packet+receipt+fresh
        return observed,self.authority.key.sign(OBSERVE+observed)
