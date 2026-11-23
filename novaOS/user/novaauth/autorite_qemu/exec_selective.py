"""Admission sélective dans UNE transaction avec la tête et les retraits.

Infrastructure QEMU opt-in, non branchée aux RPC historiques. Une inscription
n'est jamais une admission ; un reçu historique ne prouve aucune fraîcheur.
"""
from hashlib import sha256
import struct
from reprise import Refus,transaction
from recuperation import verify
import admission_exec as ex
import administration_epochs as ep
import exec_descriptor as wire

class Refusal(Refus):
 def __init__(self,status,message):super().__init__(message);self.status=status

MAX_DESCRIPTORS=64  # Borne persistante par domaine ; pas de purge implicite des IDs.
SCHEMA='''
CREATE TABLE exec_descriptors(op BLOB PRIMARY KEY,server BLOB,domain BLOB,incarnation BLOB,
 subject BLOB,effector BLOB,packet BLOB,proof BLOB,sequence INTEGER);
CREATE TABLE exec_dependencies(op BLOB,grp BLOB,revision INTEGER,
 PRIMARY KEY(op,grp,revision));
'''

def initialize(path):
 with transaction(path) as db:
  for s in SCHEMA.split(';'):
   if s.strip():db.execute(s)

def enabled(db):return db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='exec_descriptors'").fetchone() is not None

def registered(db,op):return enabled(db) and db.execute('SELECT 1 FROM exec_descriptors WHERE op=?',(op,)).fetchone() is not None

def known_effectors(db,server,domain,subject,group,revision):
 if not enabled(db):return []
 return db.execute('SELECT DISTINCT 4,d.effector FROM exec_descriptors d JOIN exec_dependencies p ON p.op=d.op WHERE d.server=? AND d.domain=? AND d.subject=? AND p.grp=? AND p.revision=?',(server,domain,subject,group,revision)).fetchall()

def unfinished(db,row,kind=None,instance=None):
 if not enabled(db) or kind not in (None,4):return False
 sql='''SELECT 1 FROM exec_descriptors d JOIN exec_dependencies p ON p.op=d.op
 JOIN exec_admissions a ON a.op=d.op WHERE d.server=? AND d.domain=? AND d.subject=?
 AND p.grp=? AND p.revision=?'''
 args=[row[k] for k in ('server','domain','subject','grp','revision')]
 if instance is not None:sql+=' AND d.effector=?';args.append(instance)
 sql+=' AND (a.state=0 OR a.terminal_sequence IS NULL OR a.terminal_sequence<=a.sequence OR a.child IS NULL OR length(a.child)!=32 OR (a.state=1 AND a.child=zeroblob(32)) OR (a.state=2 AND a.child!=zeroblob(32))) LIMIT 1'
 return db.execute(sql,args).fetchone() is not None

class Selective:
 def __init__(self,authority):self.authority=authority;self.path=authority.path;self.api=ex.Admissions(authority)
 def _active(self,db,server,domain,incarnation,generation,head,subject,epoch):
  scope=self.api._scope(db,server,domain)
  if scope[0]!=incarnation:raise Refusal(10,'REVOKED : incarnation retirée')
  if scope[3:]!=(generation,head):raise Refusal(24,'CONFLICT : tête obsolète')
  ex.guard_mutation(db,server,domain,incarnation)
  if not ep.enabled(db,server,domain):raise Refus('R2 non enrôlé')
  if db.execute('SELECT state FROM subject_epochs WHERE server=? AND domain=? AND subject=? AND epoch=?',(server,domain,subject,epoch)).fetchone()!=(ex.ACTIVE,):raise Refusal(10,'REVOKED : époque retirée')
  return scope[1]
 def _groups(self,db,d):
  from membership_retirement import available
  if not available(db):raise Refus('registre sélectif absent')
  for dep in d.dependencies:
   if dep.origin==2 and db.execute('SELECT 1 FROM membership_changes WHERE server=? AND domain=? AND subject=? AND grp=? AND revision=?',(d.server,d.domain,d.subject,dep.group,dep.membership_revision)).fetchone():
    raise Refusal(10,'REVOKED : provenance réservée ou retirée')
 def register(self,descriptor,proof):
  d=wire.Descriptor(descriptor.body)
  with transaction(self.path) as db:
   key=self._active(db,d.server,d.domain,d.incarnation,d.generation,d.head,d.subject,d.authentication_epoch)
   verify(key,wire.REGISTER,d.body,proof);self._groups(db,d)
   if self.authority._decided(db,d.operation) or db.execute('SELECT 1 FROM exec_admissions WHERE op=?',(d.operation,)).fetchone():raise Refus('identifiant déjà utilisé : consulter')
   if db.execute('SELECT count(*) FROM exec_descriptors WHERE server=? AND domain=?',(d.server,d.domain)).fetchone()[0]>=MAX_DESCRIPTORS:raise Refusal(6,'LIMIT : descripteurs persistants')
   sequence=ex.event(db,'exec-descriptor',d.body)
   db.execute('INSERT INTO exec_descriptors VALUES(?,?,?,?,?,?,?,?,?)',(d.operation,d.server,d.domain,d.incarnation,d.subject,d.effector,d.body,proof,sequence))
   db.executemany('INSERT INTO exec_dependencies VALUES(?,?,?)',[(d.operation,g,r) for g,r in sorted({(p.group,p.membership_revision) for p in d.dependencies if p.origin==2})])
  data=struct.pack('>Q',sequence)+d.body
  return data,self.authority.key.sign(wire.RECEIPT+data)
 def lookup(self,server,domain,incarnation,op,fresh,proof):
  from reprise import fixed
  packet=fixed(server,16)+fixed(domain,16)+fixed(incarnation,32)+fixed(op,16)+fixed(fresh,32)
  with transaction(self.path) as db:
   scope=self.api._scope(db,server,domain);verify(scope[1],wire.LOOKUP,packet,proof)
   # Historique accessible par l'incarnation active, sans pouvoir de réactivation.
   if scope[0]!=incarnation:raise Refus('incarnation substituée')
   row=db.execute('SELECT sequence,packet,proof FROM exec_descriptors WHERE op=? AND server=? AND domain=?',(op,server,domain)).fetchone()
   if row is None:raise Refus('descripteur absent')
   data=packet+struct.pack('>Q',row[0])+row[1]+row[2]
  return data,self.authority.key.sign(wire.LOOKUP+data)
 def _bound(self,db,q,digest):
  from reprise import fixed
  if type(q) is not ep.SubjectAdmission:raise Refus('admission R2 obligatoire')
  ep.SubjectAdmission.decode(q.packet());fixed(digest,32)
  row=db.execute('SELECT packet FROM exec_descriptors WHERE op=?',(q.op,)).fetchone()
  if row is None or sha256(row[0]).digest()!=digest:raise Refus('descripteur absent ou substitué')
  d=wire.Descriptor(row[0])
  if d.operation!=q.op:raise Refus('identifiant descripteur substitué')
  for name in ('server','domain','incarnation','owner','session','installation','image','subject','authentication_epoch'):
   if getattr(d,name)!=getattr(q,name):raise Refus('liaison ticket substituée : '+name)
  if q.generation<d.generation:raise Refus('génération restaurée')
  return d
 def admit(self,q,digest,proof):
  packet=q.packet()
  with transaction(self.path) as db:
   d=self._bound(db,q,digest)
   key=self._active(db,q.server,q.domain,q.incarnation,q.generation,q.head,q.subject,q.authentication_epoch)
   verify(key,wire.ADMIT,packet+digest,proof);self._groups(db,d)
   ep.guard_admission(db,q)
   if db.execute('SELECT 1 FROM exec_admissions WHERE op=? OR (server=? AND domain=? AND incarnation=? AND owner=? AND ticket=?)',(q.op,q.server,q.domain,q.incarnation,q.owner,q.ticket)).fetchone():raise Refus('admission déjà décidée : consulter')
   if self.authority._decided(db,q.op,exec_descriptor=True):raise Refus('identifiant déjà utilisé')
   if db.execute('SELECT count(*) FROM exec_admissions WHERE server=? AND domain=? AND incarnation=? AND state=0',(q.server,q.domain,q.incarnation)).fetchone()[0]>=ex.MAX_PENDING:raise Refusal(6,'LIMIT : admissions en attente')
   sequence=ex.event(db,'exec-selective-admission',packet+digest)
   db.execute('INSERT INTO exec_admissions VALUES(?,?,?,?,?,?,?,?,0,NULL,NULL)',(q.op,q.server,q.domain,q.incarnation,q.owner,q.ticket,packet,sequence))
   ep.link_admission(db,q)
  return self.api._receipt(sequence,packet)
 def cancel(self,q,digest,proof):
  """Tombe avant admission : preuve noyau d'annulation, jamais simple timeout."""
  data=q.finish(ex.CANCELLED,bytes(32))
  with transaction(self.path) as db:
   self._bound(db,q,digest)
   scope=self.api._scope(db,q.server,q.domain);verify(scope[1],ex.FINISH,data,proof)
   if db.execute('SELECT 1 FROM exec_admissions WHERE op=? OR (server=? AND domain=? AND incarnation=? AND owner=? AND ticket=?)',(q.op,q.server,q.domain,q.incarnation,q.owner,q.ticket)).fetchone():raise Refus('issue déjà décidée ou admission existante : consulter')
   start=ex.event(db,'exec-selective-cancel-reserved',q.packet()+digest)
   end=ex.event(db,'cancelled',data)
   db.execute('INSERT INTO exec_admissions VALUES(?,?,?,?,?,?,?,?,2,?,?)',(q.op,q.server,q.domain,q.incarnation,q.owner,q.ticket,q.packet(),start,bytes(32),end))
   ep.link_admission(db,q)
  return self.api._receipt(end,data)
