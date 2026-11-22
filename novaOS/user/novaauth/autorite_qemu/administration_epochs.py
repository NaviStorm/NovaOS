"""R2 QEMU : métadonnées de clôture, jamais copie du registre d'identités.

La tête reste publiée par Recovery dans la même transaction. Aucun transport
n'acquitte une clôture et aucune échéance n'annule une admission antérieure.
"""
from dataclasses import dataclass
import struct

from reprise import Refus, fixed, transaction
from recuperation import verify
import admission_exec as execs

ENROLL = b"NovaAuth/QEMU/administration-enroll/v1\0"
CHANGE = b"NovaAuth/QEMU/administration-change/v1\0"
EFFECTS = b"NovaAuth/QEMU/administration-effects/v1\0"
QUERY = b"NovaAuth/QEMU/administration-query/v1\0"
HISTORY = b"NovaAuth/QEMU/administration-history/v1\0"
RETIRED = b"NovaAuth/QEMU/subject-retired/v1\0"
CREATE, DISABLE, REACTIVATE, UPDATE = 1, 2, 3, 4
ALL_EFFECTS = 7  # vérificateurs, lanceur, VFS : attestations privées vérifiées par R
SCHEMA = """
CREATE TABLE admin_domains(server BLOB, domain BLOB, PRIMARY KEY(server,domain));
CREATE TABLE subject_epochs(server BLOB, domain BLOB, subject BLOB, epoch INTEGER,
 state TEXT NOT NULL CHECK(state IN ('ACTIVE','RETRAIT_EN_COURS','RETIREE')),
 request BLOB, generation INTEGER, previous BLOB,
 PRIMARY KEY(server,domain,subject,epoch));
CREATE TABLE subject_admissions(op BLOB PRIMARY KEY, server BLOB, domain BLOB, subject BLOB, epoch INTEGER);
CREATE TABLE admin_changes(op BLOB PRIMARY KEY, server BLOB, domain BLOB, subject BLOB,
 epoch INTEGER, kind INTEGER, packet BLOB, proof BLOB, effects INTEGER NOT NULL DEFAULT 0,
 terminal BLOB, terminal_signature BLOB);
"""


def initialize(path):
    with transaction(path) as db:
        for statement in SCHEMA.split(';'):
            if statement.strip():
                db.execute(statement)


def enabled(db, server, domain):
    if db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='admin_domains'").fetchone() is None:
        return False
    return db.execute("SELECT 1 FROM admin_domains WHERE server=? AND domain=?", (server, domain)).fetchone() is not None


def guard_legacy_publication(db, server, domain):
    if enabled(db, server, domain):
        raise Refus("contrat administratif requis, aucun repli historique")


@dataclass(frozen=True)
class SubjectAdmission(execs.Admission):
    subject: bytes
    authentication_epoch: int

    def packet(self):
        fixed(self.subject, 16)
        if not 0 < self.authentication_epoch < 2**63:
            raise Refus("époque d'authentification invalide")
        base = super().packet()
        return b"NAEXAD01" + base[8:] + self.subject + struct.pack('>Q', self.authentication_epoch)

    @classmethod
    def decode(cls, packet):
        if len(packet) != execs.FORMAT.size + 24 or packet[:8] != b"NAEXAD01":
            raise Refus("admission administrative invalide")
        base = execs.Admission.decode(b"NAEXEC01" + packet[8:execs.FORMAT.size])
        value = cls(**base.__dict__, subject=packet[-24:-8], authentication_epoch=int.from_bytes(packet[-8:], 'big'))
        if value.packet() != packet:
            raise Refus("encodage non canonique")
        return value


def guard_admission(db, request):
    scoped = type(request) is SubjectAdmission
    if not enabled(db, request.server, request.domain):
        if scoped:
            raise Refus("profil administratif non enrôlé")
        return
    if not scoped:
        raise Refus("époque obligatoire, admission historique refusée dans ce profil")
    # Le type canonique engage le sujet et l'époque dans le paquet signé.
    SubjectAdmission.decode(request.packet())
    row = db.execute("SELECT state FROM subject_epochs WHERE server=? AND domain=? AND subject=? AND epoch=?",
                     (request.server,request.domain,request.subject,request.authentication_epoch)).fetchone()
    if row != (execs.ACTIVE,):
        raise Refus("époque d'authentification retirée ou inconnue")


def link_admission(db, request):
    if type(request) is not SubjectAdmission:
        return
    # Annulation avant admission : une tombe peut être créée après le retrait.
    # Cela n'admet pas le ticket et n'autorise aucune création.
    if not enabled(db, request.server, request.domain):
        raise Refus("profil administratif absent")
    SubjectAdmission.decode(request.packet())
    if not db.execute("SELECT 1 FROM subject_epochs WHERE server=? AND domain=? AND subject=? AND epoch=?",
                      (request.server,request.domain,request.subject,request.authentication_epoch)).fetchone():
        raise Refus("époque inconnue")
    db.execute("INSERT INTO subject_admissions VALUES(?,?,?,?,?)",
               (request.op,request.server,request.domain,request.subject,request.authentication_epoch))


class Epochs:
    def __init__(self, recovery):
        self.authority = recovery
        self.path = recovery.path

    def _scope(self, db, state):
        if self.authority._current(db,state.server,state.domain) != state:
            raise Refus("incarnation ou tête obsolète")
        execs.guard_mutation(db,state.server,state.domain,state.instance)
        return db.execute("SELECT server_key,admin_key FROM scope WHERE server=? AND domain=?",
                          (state.server,state.domain)).fetchone()

    @staticmethod
    def enrollment(state, subjects):
        if not 1 <= len(subjects) <= 64 or len({s for s,_ in subjects}) != len(subjects):
            raise Refus("table initiale hors borne ou sujet dupliqué")
        data = bytearray(state.packet() + bytes([len(subjects)]))
        for subject, epoch in subjects:
            fixed(subject,16)
            if not 0 < epoch <= state.generation:
                raise Refus("époque initiale hors tête")
            data.extend(subject + struct.pack('>Q',epoch))
        return bytes(data)

    def enroll(self, state, subjects, administrator_proof):
        packet = self.enrollment(state,subjects)
        with transaction(self.path) as db:
            _, admin = self._scope(db,state)
            verify(admin,ENROLL,packet,administrator_proof)
            if enabled(db,state.server,state.domain):
                raise Refus("profil déjà enrôlé")
            if db.execute("SELECT 1 FROM exec_admissions WHERE server=? AND domain=? LIMIT 1",
                          (state.server,state.domain)).fetchone():
                raise Refus("activation exige un profil neuf, aucune conversion d'admission")
            db.execute("INSERT INTO admin_domains VALUES(?,?)",(state.server,state.domain))
            for subject,epoch in subjects:
                db.execute("INSERT INTO subject_epochs VALUES(?,?,?,?,?,NULL,?,?)",
                           (state.server,state.domain,subject,epoch,execs.ACTIVE,state.generation,state.head))
            seq = execs.event(db,"admin-enrollment",packet)
        data = struct.pack('>Q',seq) + packet
        return data,self.authority.key.sign(ENROLL+data)

    @staticmethod
    def change(state, op, next_head, principal, principal_epoch, subject, epoch, kind):
        fixed(op,16);fixed(next_head,32);fixed(principal,16)
        if not isinstance(subject,bytes) or len(subject)!=16 or kind not in (CREATE,DISABLE,REACTIVATE,UPDATE):
            raise Refus("mutation administrative invalide")
        if not 0 < principal_epoch < 2**63 or not 0 <= epoch < 2**63 or state.generation >= 2**63-1:
            raise Refus("époque hors plage")
        if kind != UPDATE:
            fixed(subject,16)
        return state.packet()+op+next_head+principal+struct.pack('>Q',principal_epoch)+subject+struct.pack('>QB',epoch,kind)

    def publish(self, state, op, next_head, principal, principal_epoch, subject, epoch, kind, proof):
        packet = self.change(state,op,next_head,principal,principal_epoch,subject,epoch,kind)
        with transaction(self.path) as db:
            server, _ = self._scope(db,state)
            verify(server,CHANGE,packet,proof)
            if not enabled(db,state.server,state.domain):
                raise Refus("profil non enrôlé")
            # Le rôle administratif et le contenu des trois objets sont décidés
            # par le registre isolé. L'autorité ferme ici l'époque dans son ordre.
            actor = db.execute("SELECT state FROM subject_epochs WHERE server=? AND domain=? AND subject=? AND epoch=?",
                (state.server,state.domain,principal,principal_epoch)).fetchone()
            if actor != (execs.ACTIVE,):
                raise Refus("principal inactif")
            rows = db.execute("SELECT epoch,state FROM subject_epochs WHERE server=? AND domain=? AND subject=? ORDER BY epoch DESC",
                              (state.server,state.domain,subject)).fetchall()
            if kind == CREATE:
                if rows or epoch != 0:
                    raise Refus("identité déjà utilisée")
            elif kind in (DISABLE,REACTIVATE):
                expected = execs.ACTIVE if kind == DISABLE else execs.RETIRED
                if not rows or rows[0] != (epoch,expected):
                    raise Refus("clôture non achevée ou époque divergente")
            signature = self.authority._commit_head(db,state,op,next_head)
            if kind == DISABLE:
                db.execute("UPDATE subject_epochs SET state=?,request=?,generation=?,previous=? WHERE server=? AND domain=? AND subject=? AND epoch=?",
                    (execs.DRAINING,op,state.generation+1,state.head,state.server,state.domain,subject,epoch))
            elif kind in (CREATE,REACTIVATE):
                if state.generation+1 <= epoch:
                    raise Refus("époque non monotone")
                db.execute("INSERT INTO subject_epochs VALUES(?,?,?,?,?,NULL,?,?)",
                    (state.server,state.domain,subject,state.generation+1,execs.ACTIVE,state.generation+1,state.head))
            db.execute("INSERT INTO admin_changes(op,server,domain,subject,epoch,kind,packet,proof) VALUES(?,?,?,?,?,?,?,?)",
                       (op,state.server,state.domain,subject,epoch,kind,packet,proof))
            execs.event(db,"admin-change",packet)
        return signature

    def acknowledge(self, state, op, effects, proof):
        if not 0 < effects <= ALL_EFFECTS:
            raise Refus("effecteurs invalides")
        packet = state.packet()+fixed(op,16)+bytes([effects])
        with transaction(self.path) as db:
            key, _ = self._scope(db,state)
            verify(key,EFFECTS,packet,proof)
            row = db.execute("SELECT kind,effects,terminal FROM admin_changes WHERE op=? AND server=? AND domain=?",
                             (op,state.server,state.domain)).fetchone()
            if row is None or row[0] != DISABLE or row[2] is not None or row[1] & effects == effects:
                raise Refus("retrait absent ou acquittement déjà décidé, consulter l'issue")
            db.execute("UPDATE admin_changes SET effects=? WHERE op=?",(row[1]|effects,op))
            seq = execs.event(db,"admin-effects",packet)
        data = struct.pack('>Q',seq)+packet
        return data,self.authority.key.sign(EFFECTS+data)

    def drain(self, server, domain, op):
        # Transition interne après acquittements authentifiés ; le transport ne
        # peut ni appeler un faux effecteur ni annuler une admission pour timeout.
        fixed(server,16);fixed(domain,16);fixed(op,16)
        with transaction(self.path) as db:
            row = db.execute("SELECT subject,epoch,kind,effects,terminal FROM admin_changes WHERE op=? AND server=? AND domain=?",
                             (op,server,domain)).fetchone()
            if row is None or row[2] != DISABLE or row[4] is not None:
                raise Refus("retrait absent ou terminé, consulter l'issue")
            subject,epoch,_,effects,_ = row
            r = db.execute("SELECT state,generation,previous FROM subject_epochs WHERE server=? AND domain=? AND subject=? AND epoch=? AND request=?",
                           (server,domain,subject,epoch,op)).fetchone()
            if r is None or r[0] != execs.DRAINING or effects != ALL_EFFECTS:
                raise Refus("UNCERTAIN : effecteurs non acquittés")
            missing = db.execute("SELECT 1 FROM exec_admissions e LEFT JOIN subject_admissions s ON s.op=e.op WHERE e.server=? AND e.domain=? AND s.op IS NULL LIMIT 1",
                                 (server,domain)).fetchone()
            if missing:
                raise Refus("UNCERTAIN : filiation d'admission incomplète")
            invalid = db.execute("SELECT 1 FROM subject_admissions s JOIN exec_admissions e ON s.op=e.op WHERE s.server=? AND s.domain=? AND s.subject=? AND s.epoch=? AND "
                "(e.state=0 OR e.terminal_sequence IS NULL OR e.terminal_sequence<=e.sequence OR e.child IS NULL OR length(e.child)!=32 OR "
                "(e.state=1 AND e.child=zeroblob(32)) OR (e.state=2 AND e.child!=zeroblob(32))) LIMIT 1",(server,domain,subject,epoch)).fetchone()
            if invalid:
                raise Refus("UNCERTAIN : lancement antérieur non réconcilié")
            seq = execs.event(db,"subject-retired",server+domain+subject+struct.pack('>Q',epoch)+op)
            body = server+domain+subject+struct.pack('>Q',epoch)+op+struct.pack('>QQ',r[1],seq)+bytes([1])+bytes(7)+r[2]
            assert len(body)==128
            sig = self.authority.key.sign(RETIRED+body)
            db.execute("UPDATE subject_epochs SET state=? WHERE server=? AND domain=? AND subject=? AND epoch=?",
                       (execs.RETIRED,server,domain,subject,epoch))
            db.execute("UPDATE admin_changes SET terminal=?,terminal_signature=? WHERE op=?",(body,sig,op))
        return body,sig

    def outcome(self, state, op, fresh, proof):
        # Consultation authentifiée fraîche ; n'exécute ni mutation ni drainage.
        packet = state.packet()+fixed(op,16)+fixed(fresh,32)
        with transaction(self.path) as db:
            key, _ = self._scope(db,state)
            verify(key,QUERY,packet,proof)
            row = db.execute("SELECT effects,terminal,terminal_signature FROM admin_changes WHERE op=? AND server=? AND domain=?",
                             (op,state.server,state.domain)).fetchone()
            if row is None:
                raise Refus("issue inconnue, aucune preuve de non-admission")
            data = packet+bytes([row[0]])+(row[1] or b'')+(row[2] or b'')
        return data,self.authority.key.sign(QUERY+data)

    def history(self, state, op, fresh, proof):
        """Reçu historique authentifié pour retrouver l'auteur après redémarrage.

        Consultation privée du registre ; aucun accès direct depuis un ELF.
        Le registre doit encore vérifier le principal courant et la publication.
        N'avance aucun drainage et ne republie jamais une mutation.
        """
        packet = state.packet()+fixed(op,16)+fixed(fresh,32)
        with transaction(self.path) as db:
            key, _ = self._scope(db,state)
            verify(key,HISTORY,packet,proof)
            row = db.execute("SELECT packet,effects,terminal,terminal_signature FROM admin_changes WHERE op=? AND server=? AND domain=?",
                             (op,state.server,state.domain)).fetchone()
            if row is None:
                raise Refus("issue inconnue, aucune preuve de non-admission")
            original, effects, terminal, signature = row
            if len(original)!=209 or original[:32]!=packet[:32] or original[112:128]!=op:
                raise Refus("historique incohérent")
            # La décision déjà durable est attestée dans la réponse fraîche.
            # Ni signature d'une nouvelle publication ni nouvel effet métier.
            data=packet+original+bytes([effects])+(terminal or b'')+(signature or b'')
        return data,self.authority.key.sign(HISTORY+data)
