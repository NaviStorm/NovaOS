"""Retrait sélectif privé R2 : réservations et faits durables, sans effet VFS.

L'appelant est le registre authentifié, qui vérifie les rôles et atteste les
faits des effecteurs via leurs conversations privées. Ce module ne constitue
pas, à lui seul, une preuve que ces effecteurs invités ont été raccordés.
"""
from dataclasses import dataclass
import struct
from reprise import Refus, fixed, transaction
from recuperation import verify
import admission_exec as ex
from administration_epochs import Epochs, enabled

PREFIX = b'NovaAuth/QEMU/membership/'
RESERVE, PUBLISH, EFFECT, ADMIT, FINISH, QUERY, PENDING = [PREFIX+x+b'/v1\0' for x in
    (b'reserve',b'publish',b'effect',b'admit',b'finish',b'query',b'pending')]
HISTORY = PREFIX+b'history/v1\0'
RECEIPT = b'NovaAuth/QEMU/membership-retired/v1'
RESERVED, DRAINING, RETIRED = 0, 1, 2
MAX_EFFECTORS, MAX_PENDING = 32, 64
SCHEMA = '''
CREATE TABLE membership_changes(op BLOB PRIMARY KEY, server BLOB, domain BLOB,
 incarnation BLOB, subject BLOB, grp BLOB, revision INTEGER, generation INTEGER,
 previous BLOB, next_head BLOB, principal BLOB, principal_epoch INTEGER,
 packet BLOB, proof BLOB, state INTEGER CHECK(state IN (0,1,2)), sequence INTEGER,
 p9 INTEGER NOT NULL DEFAULT 0);
CREATE UNIQUE INDEX membership_source ON membership_changes(server,domain,subject,grp,revision);
CREATE TABLE membership_effectors(op BLOB, kind INTEGER CHECK(kind IN (1,2,4)), instance BLOB,
 acknowledged INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(op,kind,instance));
CREATE TABLE membership_operations(op BLOB PRIMARY KEY, server BLOB, domain BLOB,
 incarnation BLOB, subject BLOB, grp BLOB, revision INTEGER, kind INTEGER, instance BLOB,
 packet BLOB, sequence INTEGER, state INTEGER CHECK(state IN (0,1,2)), evidence BLOB,
 terminal_sequence INTEGER);
'''


def initialize(path):
    with transaction(path) as db:
        for statement in SCHEMA.split(';'):
            if statement.strip(): db.execute(statement)


def available(db):
    return db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='membership_changes'").fetchone() is not None


def used_operation(db,op,*,reservation=True):
    if not available(db): return False
    if reservation and db.execute('SELECT 1 FROM membership_changes WHERE op=?',(op,)).fetchone(): return True
    return db.execute('SELECT 1 FROM membership_operations WHERE op=?',(op,)).fetchone() is not None


def guard_drained(db,server,domain,*,incarnation=None,subject=None,epoch=None):
    if not available(db): return
    sql='SELECT 1 FROM membership_operations WHERE server=? AND domain=?'
    args=[server,domain]
    if incarnation is not None: sql+=' AND incarnation=?';args.append(incarnation)
    if subject is not None: sql+=' AND subject=?';args.append(subject)
    if epoch is not None:
        # L'époque est canonique dans ADMIT aux octets [201:209].
        sql+=' AND substr(packet,202,8)=?';args.append(number(epoch,1))
    sql+=' AND (state=0 OR terminal_sequence IS NULL OR terminal_sequence<=sequence OR evidence IS NULL OR length(evidence)!=32 OR (state=1 AND evidence=zeroblob(32)) OR (state=2 AND evidence!=zeroblob(32))) LIMIT 1'
    if db.execute(sql,args).fetchone():
        raise Refus('UNCERTAIN : opération de groupe non réconciliée')


def guard_activation(db,server,domain):
    guard_drained(db,server,domain)
    if available(db) and db.execute('SELECT 1 FROM membership_changes WHERE server=? AND domain=? AND state!=2',(server,domain)).fetchone():
        raise Refus('UNCERTAIN : drainage sélectif incomplet, nouvelle incarnation inactive')


def guard_publication(db, state, op, next_head, selective=False):
    if not available(db):
        if selective: raise Refus('retrait sélectif non initialisé')
        return
    rows = db.execute('SELECT op,state,previous,next_head,generation FROM membership_changes WHERE server=? AND domain=? AND state!=2',
                      (state.server,state.domain)).fetchall()
    if not rows:
        if selective: raise Refus('réservation absente')
        return
    if not selective or rows != [(op,RESERVED,state.head,next_head,state.generation+1)]:
        raise Refus('UNCERTAIN : retrait sélectif réservé, publication concurrente refusée')


def number(n, minimum=0):
    if type(n) is not int or not minimum <= n < 2**63: raise Refus('entier hors borne')
    return struct.pack('>Q',n)


@dataclass(frozen=True)
class Source:
    subject: bytes
    group: bytes
    revision: int
    def packet(self):
        return fixed(self.subject,16)+fixed(self.group,16)+number(self.revision)
    def values(self):
        self.packet()
        return self.subject,self.group,self.revision


def effectors(values):
    if not 1 <= len(values) <= MAX_EFFECTORS:
        raise Refus('LIMIT : inventaire effecteurs requis et borné')
    canonical=tuple(sorted(values))
    if tuple(values)!=canonical or len(set(values))!=len(values):
        raise Refus('inventaire non canonique')
    for kind,instance in values:
        if kind not in (1,2,4): raise Refus('classe effecteur invalide')
        fixed(instance,32)
    if not any(kind==1 for kind,_ in values): raise Refus('effecteur de publication P9 requis')
    return b''.join(bytes([kind])+instance for kind,instance in values)


class Memberships:
    def __init__(self,authority):
        self.authority=authority; self.path=authority.path; self.epochs=Epochs(authority)

    def _actor(self,db,state,principal,epoch):
        key,_=self.epochs._scope(db,state)
        if not enabled(db,state.server,state.domain): raise Refus('R2 non enrôlé')
        row=db.execute('SELECT state FROM subject_epochs WHERE server=? AND domain=? AND subject=? AND epoch=?',
                       (state.server,state.domain,fixed(principal,16),int.from_bytes(number(epoch,1),'big'))).fetchone()
        if row!=(ex.ACTIVE,): raise Refus('principal retiré')
        return key

    def _reconcile_key(self,db,state):
        if self.authority._current(db,state.server,state.domain)!=state:
            raise Refus('incarnation ou tête obsolète')
        return db.execute('SELECT server_key FROM scope WHERE server=? AND domain=?',(state.server,state.domain)).fetchone()[0]

    @staticmethod
    def reservation(state,op,next_head,principal,epoch,source,expected):
        return state.packet()+fixed(op,16)+fixed(next_head,32)+fixed(principal,16)+number(epoch,1)+source.packet()+bytes([len(expected)])+effectors(expected)

    def reserve(self,state,op,next_head,principal,epoch,source,expected,proof):
        packet=self.reservation(state,op,next_head,principal,epoch,source,expected)
        if source.revision>state.generation or state.generation>=2**63-1: raise Refus("révision hors tête")
        with transaction(self.path) as db:
            verify(self._actor(db,state,principal,epoch),RESERVE,packet,proof)
            if self.authority._decided(db,op) or db.execute('SELECT 1 FROM exec_admissions WHERE op=?',(op,)).fetchone() or db.execute('SELECT 1 FROM membership_operations WHERE op=?',(op,)).fetchone():
                raise Refus('identifiant déjà utilisé : consulter')
            if db.execute('SELECT 1 FROM membership_changes WHERE op=? OR (server=? AND domain=? AND (state!=2 OR (subject=? AND grp=? AND revision=?)))',
                          (op,state.server,state.domain,*source.values())).fetchone():
                raise Refus('réservation déjà décidée ou retrait en cours : consulter')
            known=db.execute('SELECT DISTINCT kind,instance FROM membership_operations WHERE server=? AND domain=? AND subject=? AND grp=? AND revision=?',
                             (state.server,state.domain,*source.values())).fetchall()
            if any(e not in expected for e in known): raise Refus('inventaire effecteurs incomplet')
            sequence=ex.event(db,'membership-reserved',packet)
            db.execute('INSERT INTO membership_changes VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,0)',
                       (op,state.server,state.domain,state.instance,*source.values(),state.generation+1,state.head,next_head,principal,epoch,packet,proof,RESERVED,sequence))
            db.executemany('INSERT INTO membership_effectors VALUES(?,?,?,0)',[(op,k,i) for k,i in expected])
        return sequence

    def publish(self,state,op,proof):
        packet=state.packet()+fixed(op,16)
        with transaction(self.path) as db:
            row=self._row(db,state,op)
            verify(self._actor(db,state,row['principal'],row['principal_epoch']),PUBLISH,packet,proof)
            signature=self.authority._commit_head(db,state,op,row['next_head'],membership=True)
            sequence=ex.event(db,'membership-withdrawing',packet)
            db.execute('UPDATE membership_changes SET state=1,sequence=? WHERE op=?',(sequence,op))
        return signature

    @staticmethod
    def admission(state,source,kind,instance,op,authentication_epoch,session):
        if kind not in (1,2,4): raise Refus('classe invalide')
        return state.packet()+source.packet()+bytes([kind])+fixed(instance,32)+fixed(op,16)+number(authentication_epoch,1)+fixed(session,32)

    def admit(self,state,source,kind,instance,op,authentication_epoch,session,proof):
        packet=self.admission(state,source,kind,instance,op,authentication_epoch,session)
        with transaction(self.path) as db:
            key,_=self.epochs._scope(db,state);verify(key,ADMIT,packet,proof)
            if not enabled(db,state.server,state.domain): raise Refus('R2 non enrôlé')
            subject=db.execute('SELECT state FROM subject_epochs WHERE server=? AND domain=? AND subject=? AND epoch=?',
                               (state.server,state.domain,source.subject,authentication_epoch)).fetchone()
            if subject!=(ex.ACTIVE,): raise Refus("époque d'authentification retirée ou inconnue")
            if source.revision>state.generation: raise Refus('provenance future')
            # Les barrières d'époque/incarnation restent cumulatives. L'effecteur
            # revalide la session, son époque et les permissions avant cet appel.
            row=db.execute('SELECT op,state FROM membership_changes WHERE server=? AND domain=? AND subject=? AND grp=? AND revision=?',
                           (state.server,state.domain,*source.values())).fetchone()
            if row is not None:
                if row[1]!=RESERVED: raise Refus('REVOKED : provenance retirée')
                if not db.execute('SELECT 1 FROM membership_effectors WHERE op=? AND kind=? AND instance=?',(row[0],kind,instance)).fetchone():
                    raise Refus('nouvel effecteur pendant réservation')
            if self.authority._decided(db,op) or db.execute('SELECT 1 FROM exec_admissions WHERE op=?',(op,)).fetchone() or db.execute('SELECT 1 FROM membership_changes WHERE op=?',(op,)).fetchone() or db.execute('SELECT 1 FROM membership_operations WHERE op=?',(op,)).fetchone():
                raise Refus('opération déjà décidée : consulter')
            if db.execute('SELECT count(*) FROM membership_operations WHERE server=? AND domain=? AND state=0',(state.server,state.domain)).fetchone()[0]>=MAX_PENDING:
                raise Refus('LIMIT : admissions en attente')
            sequence=ex.event(db,'membership-admitted',packet)
            db.execute('INSERT INTO membership_operations VALUES(?,?,?,?,?,?,?,?,?,?,?,0,NULL,NULL)',
                       (op,state.server,state.domain,state.instance,*source.values(),kind,instance,packet,sequence))
        return sequence

    def finish(self,state,op,kind,instance,outcome,evidence,proof):
        if kind not in (1,2,4) or outcome not in (1,2): raise Refus('issue non terminale')
        if not isinstance(evidence,bytes) or len(evidence)!=32 or (outcome==1 and not any(evidence)) or (outcome==2 and any(evidence)):
            raise Refus('preuve terminale invalide')
        packet=state.packet()+fixed(op,16)+bytes([kind])+fixed(instance,32)+bytes([outcome])+evidence
        with transaction(self.path) as db:
            key=self._reconcile_key(db,state);verify(key,FINISH,packet,proof)
            row=db.execute('SELECT state,kind,instance,incarnation FROM membership_operations WHERE op=? AND server=? AND domain=?',(op,state.server,state.domain)).fetchone()
            if row!=(0,kind,instance,state.instance): raise Refus('opération absente, substituée ou déjà décidée : consulter')
            sequence=ex.event(db,'membership-operation-finished',packet)
            db.execute('UPDATE membership_operations SET state=?,evidence=?,terminal_sequence=? WHERE op=?',(outcome,evidence,sequence,op))
        return sequence

    def acknowledge(self,state,op,kind,instance,proof):
        if kind not in (1,2,4): raise Refus('classe effecteur invalide')
        packet=state.packet()+fixed(op,16)+bytes([kind])+fixed(instance,32)
        with transaction(self.path) as db:
            row=self._row(db,state,op)
            # Après retrait de l'administrateur, la réconciliation reste possible
            # par le registre authentifié ; elle ne soumet aucune nouvelle mutation.
            key=self._reconcile_key(db,state);verify(key,EFFECT,packet,proof)
            if row['incarnation']!=state.instance or row['state']!=DRAINING or state.generation<row['generation']: raise Refus('publication non démontrée')
            if state.generation==row['generation'] and state.head!=row['next_head']: raise Refus('tête divergente')
            expected=db.execute('SELECT acknowledged FROM membership_effectors WHERE op=? AND kind=? AND instance=?',(op,kind,instance)).fetchone()
            if expected!=(0,): raise Refus('effecteur substitué ou acquittement déjà décidé : consulter')
            if self._unfinished(db,row,kind,instance): raise Refus('UNCERTAIN : opérations antérieures non réconciliées')
            # Classe VFS : attestation du registre après relecture P9 ET fermeture
            # réelle de cette instance. Un transport ne doit jamais signer ceci.
            db.execute('UPDATE membership_effectors SET acknowledged=1 WHERE op=? AND kind=? AND instance=?',(op,kind,instance))
            if kind==1: db.execute('UPDATE membership_changes SET p9=1 WHERE op=?',(op,))
            sequence=ex.event(db,'membership-effector-ack',packet)
            missing=db.execute('SELECT 1 FROM membership_effectors WHERE op=? AND acknowledged=0',(op,)).fetchone()
            p9=db.execute('SELECT p9 FROM membership_changes WHERE op=?',(op,)).fetchone()[0]
            if not missing and p9 and not self._unfinished(db,row):
                sequence=ex.event(db,'membership-retired',packet)
                db.execute('UPDATE membership_changes SET state=2 WHERE op=?',(op,))
            db.execute('UPDATE membership_changes SET sequence=? WHERE op=?',(sequence,op))
        return sequence

    @staticmethod
    def _unfinished(db,row,kind=None,instance=None):
        sql='SELECT 1 FROM membership_operations WHERE server=? AND domain=? AND subject=? AND grp=? AND revision=?'
        args=[row[k] for k in ('server','domain','subject','grp','revision')]
        if kind is not None: sql+=' AND kind=? AND instance=?';args.extend((kind,instance))
        sql+=' AND (state=0 OR terminal_sequence IS NULL OR terminal_sequence<=sequence OR evidence IS NULL OR length(evidence)!=32 OR (state=1 AND evidence=zeroblob(32)) OR (state=2 AND evidence!=zeroblob(32))) LIMIT 1'
        return db.execute(sql,args).fetchone() is not None

    @staticmethod
    def _row(db,state,op):
        cursor=db.execute('SELECT * FROM membership_changes WHERE op=? AND server=? AND domain=?',(op,state.server,state.domain))
        data=cursor.fetchone()
        if data is None: raise Refus('retrait inconnu, aucune preuve de non-admission')
        return dict(zip((c[0] for c in cursor.description),data))

    def outcome(self,state,op,fresh,proof):
        packet=state.packet()+fixed(op,16)+fixed(fresh,32)
        with transaction(self.path) as db:
            key=self._reconcile_key(db,state);verify(key,QUERY,packet,proof)
            row=self._row(db,state,op)
            expected=acknowledged=0
            for kind in (1,2,4):
                flags=db.execute('SELECT acknowledged FROM membership_effectors WHERE op=? AND kind=?',(op,kind)).fetchall()
                if flags:
                    expected|=kind
                    if all(x==(1,) for x in flags): acknowledged|=kind
            body=(b'NAMBRE01'+state.server+state.domain+row['incarnation']+op+row['subject']+row['grp']+
                  number(row['revision'])+number(row['generation'])+row['previous']+row['next_head']+
                  number(row['sequence'],1)+bytes([row['state'],expected,acknowledged])+bytes(5)+fresh)
            assert len(body)==248
        return body,self.authority.key.sign(RECEIPT+body)

    def pending(self,state,fresh,proof):
        """Consultation obligatoire avant réouverture après perte de l'état volatil.

        Une tête exacte ne suffit pas à exclure une réservation encore sans
        publication. Lecture pure, un seul retrait en cours dans ce profil.
        """
        packet=state.packet()+fixed(fresh,32)
        with transaction(self.path) as db:
            key=self._reconcile_key(db,state);verify(key,PENDING,packet,proof)
            rows=db.execute('SELECT op,subject,grp,revision,state FROM membership_changes WHERE server=? AND domain=? AND state!=2',
                            (state.server,state.domain)).fetchall()
            if len(rows)>1: raise Refus('historique de réservation incohérent')
            body=packet+bytes([len(rows)])
            for op,subject,group,revision,status in rows:
                body+=op+subject+group+number(revision)+bytes([status])
        return body,self.authority.key.sign(PENDING+body)

    def operation_outcome(self,state,op,fresh,proof):
        """Issue individuelle après réponse FINISH perdue ; lecture sans mutation.

        Le corps contient la requête originale complète (session et époque
        comprises), la position d'admission et l'issue exacte avec son fait.
        """
        packet=state.packet()+fixed(op,16)+fixed(fresh,32)
        with transaction(self.path) as db:
            key=self._reconcile_key(db,state);verify(key,QUERY,packet,proof)
            row=db.execute('SELECT packet,sequence,state,evidence,terminal_sequence FROM membership_operations WHERE op=? AND server=? AND domain=?',
                           (op,state.server,state.domain)).fetchone()
            if row is None: raise Refus('opération inconnue, aucune preuve de non-admission')
            original,position,status,evidence,terminal=row
            if len(original)!=241: raise Refus('historique incomplet')
            body=b'NAMBOU01'+packet+original+number(position,1)+bytes([status])+(evidence or bytes(32))+number(terminal or 0)
        return body,self.authority.key.sign(QUERY+body)


    def history(self,state,op,fresh,proof):
        """Réservation canonique et reçu signé : lecture pure, sans nouvel ACK.

        Le registre contrôle le principal courant avant de divulguer l'issue.
        Une incarnation neuve ne réécrit jamais l'identité des effecteurs anciens.
        """
        packet=state.packet()+fixed(op,16)+fixed(fresh,32)
        with transaction(self.path) as db:
            key=self._reconcile_key(db,state);verify(key,HISTORY,packet,proof)
            row=self._row(db,state,op);original=row['packet']
            if len(original)<258 or len(original)!=225+33*original[224] or not 1<=original[224]<=32:
                raise Refus('historique de réservation incomplet')
            if original[:32]!=packet[:32] or original[112:128]!=op or original[160:176]!=row['principal']:
                raise Refus('historique de réservation incohérent')
            expected=acknowledged=0
            for kind in (1,2,4):
                flags=db.execute('SELECT acknowledged FROM membership_effectors WHERE op=? AND kind=?',(op,kind)).fetchall()
                if flags:
                    expected|=kind
                    if all(x==(1,) for x in flags):acknowledged|=kind
            receipt=(b'NAMBRE01'+state.server+state.domain+row['incarnation']+op+row['subject']+row['grp']+
                number(row['revision'])+number(row['generation'])+row['previous']+row['next_head']+
                number(row['sequence'],1)+bytes([row['state'],expected,acknowledged])+bytes(5)+fresh)
            receipt+=self.authority.key.sign(RECEIPT+receipt)
            body=b'NAMBHI01'+packet+len(original).to_bytes(2,'big')+original+receipt
        return body,self.authority.key.sign(HISTORY+body)
