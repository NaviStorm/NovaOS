"""Admissions ELF et drainage durables de laboratoire, jamais un décideur invité.

Une issue terminale signée atteste un fait du noyau invité : enfant publié ou
annulation irréversible. Ni le transport, ni une échéance ne peuvent la fabriquer.
L'API n'est pas encore un raccord de consommation SPAWN : celui-ci doit fournir
ces attestations depuis le chemin privé du registre.
"""
from dataclasses import dataclass
from hashlib import sha256
import struct

from reprise import Refus, fixed, transaction
from recuperation import verify

PREFIX = b"NovaAuth/QEMU/exec-admission/v1\0"
ADMIT, FINISH, WITHDRAW, QUERY, ENROLL = [PREFIX + s + b"\0" for s in
    (b"admit", b"finish", b"withdraw", b"query", b"enroll")]
RECEIPT = PREFIX + b"receipt\0"
ACTIVE = "ACTIVE"
DRAINING = "RETRAIT_EN_COURS"
RETIRED = "RETIREE"
PENDING, CREATED, CANCELLED = 0, 1, 2
MAX_PENDING = 64
FORMAT = struct.Struct(">8s16s16s32sQ32s32s32s16s32s16s16s")
SCHEMA = """
CREATE TABLE exec_routes(server BLOB, domain BLOB, incarnation BLOB,
 state TEXT NOT NULL CHECK(state IN ('ACTIVE','RETRAIT_EN_COURS','RETIREE')),
 PRIMARY KEY(server,domain,incarnation));
CREATE TABLE exec_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,
 kind TEXT NOT NULL, packet BLOB NOT NULL);
CREATE TABLE exec_admissions(op BLOB PRIMARY KEY, server BLOB, domain BLOB,
 incarnation BLOB, owner BLOB, ticket BLOB, packet BLOB NOT NULL,
 sequence INTEGER NOT NULL, state INTEGER NOT NULL CHECK(state IN (0,1,2)),
 child BLOB, terminal_sequence INTEGER,
 UNIQUE(server,domain,incarnation,owner,ticket));
"""


def initialize(path):
    with transaction(path) as db:
        # executescript commits implicitly: individual statements preserve atomicity.
        for statement in SCHEMA.split(';'):
            if statement.strip():
                db.execute(statement)


def event(db, kind, packet):
    return db.execute("INSERT INTO exec_events(kind,packet) VALUES(?,?)",
                      (kind, packet)).lastrowid


def enabled(db):
    return db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='exec_routes'").fetchone() is not None


def guard_activation(db, server, domain, old, new):
    """Called inside the SAME transaction as replacement of scope.instance."""
    if not enabled(db):
        return  # Historical QEMU fixture, no EXEC admission surface installed.
    row = db.execute("SELECT state FROM exec_routes WHERE server=? AND domain=? AND incarnation=?",
                     (server, domain, old)).fetchone()
    if row is None:
        raise Refus("registre d'admission non provisionné")
    if row != (RETIRED,):
        raise Refus("drainage EXEC non confirmé : nouvelle incarnation inactive")
    db.execute("INSERT INTO exec_routes VALUES(?,?,?,?)", (server, domain, new, ACTIVE))
    event(db, "activation", server + domain + old + new)


def guard_mutation(db, server, domain, incarnation):
    if enabled(db):
        row = db.execute("SELECT state FROM exec_routes WHERE server=? AND domain=? AND incarnation=?",
                         (server, domain, incarnation)).fetchone()
        if row != (ACTIVE,):
            raise Refus("route en retrait ou non provisionnée")


@dataclass(frozen=True)
class Admission:
    server: bytes
    domain: bytes
    incarnation: bytes
    generation: int
    head: bytes
    owner: bytes  # boot-qualified instance identity, not a reusable numeric PID
    session: bytes
    installation: bytes
    image: bytes
    ticket: bytes
    op: bytes

    def packet(self):
        fields = ((self.server,16),(self.domain,16),(self.incarnation,32),
                  (self.head,32),(self.owner,32),(self.session,32),(self.installation,16),
                  (self.image,32),(self.ticket,16),(self.op,16))
        values=[fixed(v,n) for v,n in fields]
        if not 0 <= self.generation < 2**63:
            raise Refus("génération hors plage")
        return FORMAT.pack(b"NAEXEC01", *values[:3],self.generation,*values[3:])

    @classmethod
    def decode(cls, packet):
        if len(packet) != FORMAT.size:
            raise Refus("taille admission invalide")
        magic, *fields = FORMAT.unpack(packet)
        value = cls(*fields)
        if magic != b"NAEXEC01" or value.packet() != packet:
            raise Refus("version ou encodage admission invalide")
        return value

    def finish(self, state, child):
        if state not in (CREATED, CANCELLED):
            raise Refus("issue non terminale")
        if not isinstance(child, bytes) or len(child) != 32 or (any(child) != (state == CREATED)):
            raise Refus("identité enfant incompatible avec l'issue")
        return sha256(self.packet()).digest() + bytes([state]) + child


class Admissions:
    def __init__(self, recovery):
        self.authority = recovery
        self.path = recovery.path

    def _scope(self, db, server, domain):
        row = db.execute("SELECT instance,server_key,admin_key,generation,head FROM scope WHERE server=? AND domain=?",
                         (fixed(server,16),fixed(domain,16))).fetchone()
        if row is None:
            raise Refus("domaine inconnu")
        return row

    def enroll(self, server, domain, incarnation, signature):
        # Explicit administrator action; no implicit migration of an old authority.
        data = fixed(server,16)+fixed(domain,16)+fixed(incarnation,32)
        with transaction(self.path) as db:
            scope = self._scope(db,server,domain)
            verify(scope[2], ENROLL, data, signature)
            if scope[0] != incarnation or db.execute(
                "SELECT 1 FROM exec_routes WHERE server=? AND domain=?",(server,domain)).fetchone():
                raise Refus("route absente ou déjà provisionnée")
            db.execute("INSERT INTO exec_routes VALUES(?,?,?,?)",(server,domain,incarnation,ACTIVE))
            seq = event(db,"enrollment",data)
        return self._receipt(seq, data)

    def _receipt(self, seq, data):
        packet = struct.pack(">Q", seq) + data
        return packet, self.authority.key.sign(RECEIPT + packet)

    def admit(self, request, proof):
        packet = request.packet()
        with transaction(self.path) as db:
            scope = self._scope(db,request.server,request.domain)
            verify(scope[1],ADMIT,packet,proof)
            if scope[0] != request.incarnation:
                raise Refus("incarnation retirée")
            if scope[3:] != (request.generation,request.head):
                raise Refus("génération ou empreinte obsolète")
            guard_mutation(db,request.server,request.domain,request.incarnation)
            if db.execute("SELECT 1 FROM exec_admissions WHERE op=? OR "
                "(server=? AND domain=? AND incarnation=? AND owner=? AND ticket=?)",
                (request.op,request.server,request.domain,request.incarnation,request.owner,request.ticket)).fetchone():
                raise Refus("admission déjà décidée : consulter l'issue")
            if self.authority._decided(db,request.op):
                raise Refus("identifiant d'opération déjà utilisé")
            count = db.execute("SELECT count(*) FROM exec_admissions WHERE server=? AND domain=? AND incarnation=? AND state=0",
                (request.server,request.domain,request.incarnation)).fetchone()[0]
            if count >= MAX_PENDING:
                raise Refus("LIMIT : admissions en attente")
            seq = event(db,"admission",packet)
            db.execute("INSERT INTO exec_admissions VALUES(?,?,?,?,?,?,?,?,0,NULL,NULL)",
                (request.op,request.server,request.domain,request.incarnation,request.owner,request.ticket,packet,seq))
        # Receipt only leaves the authority after COMMIT has completed.
        return self._receipt(seq, packet)

    def finish(self, request, state, child, proof):
        data = request.finish(state,child)
        with transaction(self.path) as db:
            scope = self._scope(db,request.server,request.domain)
            verify(scope[1],FINISH,data,proof)
            row = db.execute("SELECT packet,state,child,terminal_sequence FROM exec_admissions WHERE op=?",
                             (request.op,)).fetchone()
            if row is None and state == CANCELLED:
                # Une demande signée peut encore être en transit. Une lecture
                # « inconnue » ne l'annule pas : conserver une tombe durable.
                if self.authority._decided(db,request.op) or db.execute(
                    "SELECT 1 FROM exec_admissions WHERE server=? AND domain=? AND incarnation=? AND owner=? AND ticket=?",
                    (request.server,request.domain,request.incarnation,request.owner,request.ticket)).fetchone():
                    raise Refus("identifiant déjà utilisé")
                reservation=event(db,"cancel-before-admission",request.packet())
                db.execute("INSERT INTO exec_admissions VALUES(?,?,?,?,?,?,?,?,0,NULL,NULL)",
                    (request.op,request.server,request.domain,request.incarnation,request.owner,request.ticket,request.packet(),reservation))
                row=(request.packet(),PENDING,None,None)
            if row is None or row[0] != request.packet():
                raise Refus("admission inconnue ou substituée")
            if row[1] != PENDING:
                raise Refus("issue déjà décidée : consulter, ne pas répéter")
            seq = event(db,"created" if state == CREATED else "cancelled",data)
            db.execute("UPDATE exec_admissions SET state=?,child=?,terminal_sequence=? WHERE op=?",
                       (state,child,seq,request.op))
        return self._receipt(seq,data)

    def begin_withdrawal(self, server, domain, incarnation, proof):
        data = fixed(server,16)+fixed(domain,16)+fixed(incarnation,32)
        with transaction(self.path) as db:
            scope = self._scope(db,server,domain)
            verify(scope[2],WITHDRAW,data,proof)
            if scope[0] != incarnation:
                raise Refus("route substituée")
            guard_mutation(db,server,domain,incarnation)
            seq = event(db,DRAINING,data)
            db.execute("UPDATE exec_routes SET state=? WHERE server=? AND domain=? AND incarnation=?",
                       (DRAINING,server,domain,incarnation))
        # This is NOT a receipt of definitive withdrawal.
        return self._receipt(seq,DRAINING.encode()+b'\0'+data)

    def drain(self, server, domain, incarnation):
        # Internal authority transition: cannot cancel any outstanding admission.
        data = fixed(server,16)+fixed(domain,16)+fixed(incarnation,32)
        with transaction(self.path) as db:
            row = db.execute("SELECT state FROM exec_routes WHERE server=? AND domain=? AND incarnation=?",
                             (server,domain,incarnation)).fetchone()
            if row != (DRAINING,):
                raise Refus("retrait absent ou déjà terminé")
            if db.execute("SELECT 1 FROM exec_admissions WHERE server=? AND domain=? AND incarnation=? AND state=0 LIMIT 1",
                          (server,domain,incarnation)).fetchone():
                raise Refus("UNCERTAIN : drainage incomplet, B inactive")
            # Contrôler aussi les issues terminales : un état sans identité
            # enfant ou sans décision ordonnée n'est pas une preuve de drainage.
            invalid = db.execute("SELECT 1 FROM exec_admissions WHERE server=? AND domain=? AND incarnation=? AND "
                "(terminal_sequence IS NULL OR terminal_sequence<=sequence OR child IS NULL OR length(child)!=32 OR "
                "(state=1 AND child=zeroblob(32)) OR (state=2 AND child!=zeroblob(32))) LIMIT 1",
                (server,domain,incarnation)).fetchone()
            if invalid:
                raise Refus("UNCERTAIN : issue terminale incomplète, B inactive")
            seq = event(db,RETIRED,data)
            db.execute("UPDATE exec_routes SET state=? WHERE server=? AND domain=? AND incarnation=?",
                       (RETIRED,server,domain,incarnation))
        return self._receipt(seq,RETIRED.encode()+b'\0'+data)

    def query(self, request, fresh, proof):
        fresh = fixed(fresh,32)
        packet = request.packet()
        with transaction(self.path) as db:
            scope = self._scope(db,request.server,request.domain)
            verify(scope[1],QUERY,packet+fresh,proof)
            row = db.execute("SELECT packet,sequence,state,child,terminal_sequence FROM exec_admissions WHERE op=?",
                             (request.op,)).fetchone()
            if row is None or row[0] != packet:
                raise Refus("admission inconnue ou substituée")
            data = packet+fresh+struct.pack(">QB32sQ",row[1],row[2],row[3] or bytes(32),row[4] or 0)
        return data, self.authority.key.sign(QUERY+data)

    def withdrawal_outcome(self, server, domain, incarnation, fresh, proof):
        """Read-only reconciliation; never repeats withdrawal or cancels a launch."""
        data=fixed(server,16)+fixed(domain,16)+fixed(incarnation,32)
        fresh=fixed(fresh,32)
        with transaction(self.path) as db:
            scope=self._scope(db,server,domain)
            verify(scope[2],QUERY,data+fresh,proof)
            row=db.execute("SELECT state FROM exec_routes WHERE server=? AND domain=? AND incarnation=?",
                           (server,domain,incarnation)).fetchone()
            if row is None:
                raise Refus("incarnation inconnue")
            pending=db.execute("SELECT op FROM exec_admissions WHERE server=? AND domain=? AND incarnation=? AND state=0 ORDER BY sequence",
                               (server,domain,incarnation)).fetchall()
            if len(pending)>MAX_PENDING:
                raise Refus("registre des admissions incohérent")
            record=db.execute("SELECT sequence FROM exec_events WHERE kind=? AND packet=? ORDER BY sequence DESC LIMIT 1",
                              (row[0],data)).fetchone()
            # ACTIVE is the initial administrative enrollment, not a withdrawal receipt.
            position=record[0] if record else 0
            state=row[0].encode('ascii')
            packet=data+fresh+struct.pack('>QB',position,len(state))+state+bytes([len(pending)])+b''.join(p[0] for p in pending)
        return packet,self.authority.key.sign(QUERY+packet)
