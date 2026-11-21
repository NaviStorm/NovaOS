"""Autorité de reprise QEMU. La base et le contrôle administratif résident hors de l'invité."""

from contextlib import closing, contextmanager
from dataclasses import dataclass
from hashlib import sha256
from pathlib import Path
import hmac
import os
import secrets
import sqlite3
import struct
import time

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

ADMIN = b"NovaAuth/QEMU/reprise-admin/1\0"
POSSESSION = b"NovaAuth/QEMU/registry-possession/v1\0"
RECEIPT = b"NovaAuth/QEMU/reprise-recu/1\0"
HEAD_CHANGE = b"NovaAuth/QEMU/politique-compare-publie/1\0"
HEAD_OBSERVED = b"NovaAuth/QEMU/etat-courant/1\0"
HEAD_RECEIPT = b"NovaAuth/QEMU/politique-recu/1\0"
QUERY = b"NovaAuth/QEMU/reprise-consultation/1\0"
FORMAT = struct.Struct(">16s16s32s32s16s32sQQ32s32s")
SCHEMA = """
CREATE TABLE authority(singleton INTEGER PRIMARY KEY CHECK(singleton = 1), public_key BLOB NOT NULL);
CREATE TABLE scope(server BLOB, domain BLOB, instance BLOB, epoch INTEGER,
    generation INTEGER, head BLOB, server_key BLOB, admin_key BLOB, PRIMARY KEY(server,domain));
CREATE TABLE used(server BLOB, domain BLOB, instance BLOB, PRIMARY KEY(server,domain,instance));
CREATE TABLE mutations(op BLOB PRIMARY KEY, packet BLOB NOT NULL, signature BLOB NOT NULL);
CREATE TABLE audit(op BLOB PRIMARY KEY, packet BLOB NOT NULL, admin BLOB NOT NULL,
    signature BLOB NOT NULL, admin_signature BLOB NOT NULL, possession BLOB NOT NULL);
"""


class Refus(ValueError):
    pass


def fixed(value, size):
    if not isinstance(value, bytes) or len(value) != size or not any(value):
        raise Refus("champ absent ou invalide")
    return value


@dataclass(frozen=True)
class Challenge:
    server: bytes
    domain: bytes
    old: bytes
    new: bytes
    op: bytes
    nonce: bytes
    epoch: int
    generation: int
    head: bytes
    admin: bytes

    def packet(self):
        for field, size in (
            (self.server, 16),
            (self.domain, 16),
            (self.old, 32),
            (self.new, 32),
            (self.op, 16),
            (self.nonce, 32),
            (self.head, 32),
            (self.admin, 32),
        ):
            fixed(field, size)
        if not (0 < self.epoch < 2**63 and 0 <= self.generation < 2**63):
            raise Refus("génération invalide")
        return FORMAT.pack(
            self.server,
            self.domain,
            self.old,
            self.new,
            self.op,
            self.nonce,
            self.epoch,
            self.generation,
            self.head,
            self.admin,
        )

    def possession(self):
        return sha256(
            POSSESSION + self.server + self.domain + self.new + self.nonce
        ).digest()


def initialize(path, authority_public):
    fixed(authority_public, 32)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    os.close(fd)
    with closing(open_database(path)) as db:
        db.executescript("BEGIN IMMEDIATE;" + SCHEMA)
        db.execute("INSERT INTO authority VALUES(1,?)", (authority_public,))
        db.commit()


def open_database(path):
    uri = Path(path).resolve().as_uri() + "?mode=rw"
    db = sqlite3.connect(uri, uri=True, isolation_level=None)
    db.execute("PRAGMA synchronous=EXTRA")
    db.execute("PRAGMA fullfsync=ON")
    return db


@contextmanager
def transaction(path):
    with closing(open_database(path)) as db:
        db.execute("BEGIN IMMEDIATE")
        try:
            yield db
            db.commit()
        except BaseException:
            db.rollback()
            raise


@dataclass(frozen=True)
class Published:
    server: bytes
    domain: bytes
    instance: bytes
    epoch: int
    generation: int
    head: bytes

    def packet(self):
        for value, size in (
            (self.server, 16),
            (self.domain, 16),
            (self.instance, 32),
            (self.head, 32),
        ):
            fixed(value, size)
        if not (0 <= self.epoch < 2**63 and 0 <= self.generation < 2**63):
            raise Refus("compteur hors plage")
        return struct.pack(
            ">16s16s32sQQ32s",
            self.server,
            self.domain,
            self.instance,
            self.epoch,
            self.generation,
            self.head,
        )

    def mutation(self, op, next_head):
        if self.generation == 2**63 - 1:
            raise Refus("génération épuisée")
        return HEAD_CHANGE + self.packet() + fixed(op, 16) + fixed(next_head, 32)


class Recovery:
    """Une incarnation du service externe. Les défis meurent avec le service."""

    def __init__(self, path, authority_key, clock=time.monotonic_ns):
        self.path = path
        self.public = authority_key.public_key().public_bytes_raw()
        with closing(open_database(path)) as db:
            if db.execute(
                "SELECT public_key FROM authority WHERE singleton=1"
            ).fetchone() != (self.public,):
                raise Refus("clé de reçu différente de la clé provisionnée")
        self.key = authority_key
        self.clock = clock
        # Le secret éphémère n'est jamais enregistré avec les disques du serveur.
        self.boot = secrets.token_bytes(32)

    def provision(
        self, server, domain, instance, generation, head, server_key, admin_key
    ):
        # Commande d'installation sous contrôle local de l'autorité de laboratoire.
        for v, n in (
            (server, 16),
            (domain, 16),
            (instance, 32),
            (head, 32),
            (server_key, 32),
            (admin_key, 32),
        ):
            fixed(v, n)
        if (
            len({self.public, server_key, admin_key}) != 3
            or not 0 <= generation < 2**63
        ):
            raise Refus("provisionnement incohérent")
        with transaction(self.path) as db:
            if db.execute(
                "SELECT 1 FROM scope WHERE admin_key=? OR server_key=?",
                (server_key, admin_key),
            ).fetchone():
                raise Refus("confusion entre les rôles administrateur et serveur")
            db.execute(
                "INSERT INTO scope VALUES(?,?,?,0,?,?,?,?)",
                (server, domain, instance, generation, head, server_key, admin_key),
            )
            db.execute("INSERT INTO used VALUES(?,?,?)", (server, domain, instance))

    @staticmethod
    def _decided(db, op):
        return (
            db.execute(
                "SELECT 1 FROM mutations WHERE op=? UNION ALL "
                "SELECT 1 FROM audit WHERE op=? LIMIT 1",
                (op, op),
            ).fetchone()
            is not None
        )

    def _current(self, db, server, domain):
        row = db.execute(
            "SELECT instance,epoch,generation,head FROM scope WHERE server=? AND domain=?",
            (fixed(server, 16), fixed(domain, 16)),
        ).fetchone()
        if row is None:
            raise Refus("domaine inconnu")
        return Published(server, domain, *row)

    def observe(self, server, domain, fresh):
        # Ce service interne est appelé après vérification du pair de transport.
        with transaction(self.path) as db:
            state = self._current(db, server, domain)
            return state, self.key.sign(
                HEAD_OBSERVED + fixed(fresh, 32) + state.packet()
            )

    def observe_peer(self, server, domain, fresh, peer_public):
        # Une route retirée peut découvrir sa mise en quarantaine. Cette lecture
        # ne fournit aucune admission, signature de mutation ni donnée privée.
        with transaction(self.path) as db:
            state = self._current(db, server, domain)
            stored = db.execute("SELECT server_key FROM scope WHERE server=? AND domain=?",
                                (server, domain)).fetchone()
            if stored != (fixed(peer_public, 32),):
                raise Refus("certificat hors portée")
            return state, self.key.sign(HEAD_OBSERVED + fixed(fresh, 32) + state.packet())

    def publish(self, state, op, next_head, proof):
        packet = state.mutation(op, next_head)
        with transaction(self.path) as db:
            from admission_exec import guard_mutation
            guard_mutation(db,state.server,state.domain,state.instance)
            if self._current(db, state.server, state.domain) != state:
                raise Refus("route ou tête retirée")
            if self._decided(db, op):
                raise Refus("mutation déjà décidée")
            (raw,) = db.execute(
                "SELECT server_key FROM scope WHERE server=? AND domain=?",
                (state.server, state.domain),
            ).fetchone()
            Ed25519PublicKey.from_public_bytes(raw).verify(proof, packet)
            from administration_epochs import guard_legacy_publication
            guard_legacy_publication(db, state.server, state.domain)
            signature = self._commit_head(db, state, op, next_head)
        return signature

    def _commit_head(self, db, state, op, next_head):
        """Private shared writer; the caller verifies its complete signed variant.

        Requires the caller's existing transaction. No second database writer or
        nested COMMIT: administrative metadata and the head become durable together.
        """
        from admission_exec import guard_mutation
        guard_mutation(db, state.server, state.domain, state.instance)
        if self._current(db, state.server, state.domain) != state or self._decided(db, op):
            raise Refus("route, tête ou requête déjà décidée")
        packet = state.mutation(op, next_head)
        signature = self.key.sign(HEAD_RECEIPT + packet)
        db.execute("INSERT INTO mutations VALUES(?,?,?)", (op, packet, signature))
        db.execute("UPDATE scope SET generation=?,head=? WHERE server=? AND domain=?",
                   (state.generation + 1, next_head, state.server, state.domain))
        return signature

    def mutation_outcome(self, op, fresh):
        fixed(op, 16)
        fixed(fresh, 32)
        with closing(open_database(self.path)) as db:
            row = db.execute(
                "SELECT packet,signature FROM mutations WHERE op=?", (op,)
            ).fetchone()
        if row is None:
            raise Refus("mutation inconnue")
        packet, signature = row
        return packet, signature, self.key.sign(QUERY + fresh + packet + signature)

    def _bound(self, challenge, deadline):
        return hmac.digest(
            self.boot, challenge.packet() + struct.pack(">Q", deadline), "sha256"
        )

    def issue(self, server, domain, new, op):
        fixed(new, 32)
        fixed(op, 16)
        with transaction(self.path) as db:
            row = db.execute(
                "SELECT instance,epoch,generation,head,admin_key FROM scope "
                "WHERE server=? AND domain=?",
                (server, domain),
            ).fetchone()
            if (
                not row
                or db.execute(
                    "SELECT 1 FROM used WHERE server=? AND domain=? AND instance=?",
                    (server, domain, new),
                ).fetchone()
            ):
                raise Refus("cible absente ou incarnation déjà utilisée")
            old, epoch, gen, head, admin = row
            c = Challenge(
                server,
                domain,
                old,
                new,
                op,
                secrets.token_bytes(32),
                epoch + 1,
                gen,
                head,
                admin,
            )
            deadline = self.clock() + 30_000_000_000
            return c, deadline, self._bound(c, deadline)

    def admit(self, c, deadline, cookie, signed_by_admin, signed_by_guest):
        # Le domaine de signature de possession diffère de celui de l'approbation.
        packet = c.packet()
        with transaction(self.path) as db:
            if not hmac.compare_digest(cookie, self._bound(c, deadline)):
                raise Refus("défi étranger ou émis avant redémarrage")
            row = db.execute(
                "SELECT instance,epoch,generation,head,server_key,admin_key FROM scope "
                "WHERE server=? AND domain=?",
                (c.server, c.domain),
            ).fetchone()
            if not row or (row[0], row[1] + 1, row[2], row[3], row[5]) != (
                c.old,
                c.epoch,
                c.generation,
                c.head,
                c.admin,
            ):
                raise Refus("route, génération ou identité administrative périmée")
            if (
                self._decided(db, c.op)
                or db.execute(
                    "SELECT 1 FROM used WHERE server=? AND domain=? AND instance=?",
                    (c.server, c.domain, c.new),
                ).fetchone()
            ):
                raise Refus("approbation déjà consommée")
            Ed25519PublicKey.from_public_bytes(row[4]).verify(
                signed_by_guest, c.possession()
            )
            Ed25519PublicKey.from_public_bytes(row[5]).verify(
                signed_by_admin, ADMIN + packet
            )
            if self.clock() >= deadline:
                raise Refus("défi expiré")
            from admission_exec import guard_activation
            guard_activation(db,c.server,c.domain,c.old,c.new)
            signature = self.key.sign(RECEIPT + packet)
            db.execute(
                "INSERT INTO audit VALUES(?,?,?,?,?,?)",
                (c.op, packet, c.admin, signature, signed_by_admin, signed_by_guest),
            )
            db.execute("INSERT INTO used VALUES(?,?,?)", (c.server, c.domain, c.new))
            # Seule la route change. La tête durable des droits reste celle vérifiée ci-dessus.
            db.execute(
                "UPDATE scope SET instance=?,epoch=? WHERE server=? AND domain=?",
                (c.new, c.epoch, c.server, c.domain),
            )
        return signature

    def outcome(self, op, fresh):
        fixed(op, 16)
        fixed(fresh, 32)
        with closing(open_database(self.path)) as db:
            row = db.execute(
                "SELECT packet,signature FROM audit WHERE op=?", (op,)
            ).fetchone()
        if not row:
            raise Refus("décision inconnue")
        packet, signature = row
        return packet, signature, self.key.sign(QUERY + fresh + packet + signature)
