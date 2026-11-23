"""Descripteur privé EXEC R2, immuable et borné. Aucun droit porté par son ID.

Le registre authentifié atteste les dépendances réellement déléguées. Ce codec
ne décide pas des permissions ; aucune route invitée ne l'active implicitement.
"""
from dataclasses import dataclass
import struct
from reprise import Refus, fixed

MAGIC=b'NXEXDP01'
HEADER=512
DEPENDENCY=72
MAX_DEPENDENCIES=4
MAX_SIZE=HEADER+DEPENDENCY*MAX_DEPENDENCIES
REGISTER=b'NovaAuth/QEMU/exec-descriptor/register/v1\0'
LOOKUP=b'NovaAuth/QEMU/exec-descriptor/lookup/v1\0'
ADMIT=b'NovaAuth/QEMU/exec-selective/admit/v1\0'
RECEIPT=b'NovaAuth/QEMU/exec-descriptor/receipt/v1\0'
# Offsets stables, entiers big endian, aucun champ implicite ou tronqué.
FIELDS=(('server',8,16),('domain',24,16),('incarnation',40,32),
 ('owner',72,32),('effector',104,32),('subject',136,16),('session',160,32),
 ('route',192,32),('operation',224,16),('installation',240,16),
 ('recipient',256,16),('package',272,16),('image',296,32),('volume',328,32),
 ('manifest',392,32),('head',432,32),('clock',472,32))
NUMBERS=(('authentication_epoch',152),('object',360),('object_revision',368),
 ('offset',376),('image_size',384),('generation',424),('deadline',464))

@dataclass(frozen=True)
class Dependency:
 action:int  # 1 installation personnelle nécessaire, 2 droit VFS effectivement délégué
 origin:int  # 1 Direct, 2 Group
 rights:int  # installation=0 ; VFS READ=1/WRITE=2, aucune nouvelle capacité
 owner:bytes
 object:int
 group:bytes
 membership_revision:int
 rule_revision:int
 image_revision:int
 def packet(self):
  if self.action not in (1,2) or self.origin not in (1,2):raise Refus('dépendance inconnue')
  fixed(self.owner,16)
  if type(self.object) is not int or not 0<self.object<2**63:raise Refus('objet invalide')
  for n in (self.membership_revision,self.rule_revision,self.image_revision):
   if type(n) is not int or not 0<=n<2**63:raise Refus('révision invalide')
  if not isinstance(self.group,bytes) or len(self.group)!=16:raise Refus('groupe invalide')
  if self.origin==1 and (any(self.group) or self.membership_revision):raise Refus('origine Direct contradictoire')
  if self.origin==2 and not any(self.group):raise Refus('origine Group absente')
  if self.action==1 and (self.origin!=1 or self.rights or self.rule_revision):raise Refus('installation non canonique')
  if self.action==2 and self.rights not in (1,2,3):raise Refus('droits VFS invalides')
  return struct.pack('>BBB5x16sQ16sQQQ',self.action,self.origin,self.rights,self.owner,self.object,self.group,self.membership_revision,self.rule_revision,self.image_revision)
 @classmethod
 def decode(cls,p):
  if len(p)!=DEPENDENCY:raise Refus('taille dépendance')
  d=cls(*struct.unpack('>BBB5x16sQ16sQQQ',p))
  if d.packet()!=p:raise Refus('dépendance non canonique')
  return d
 def key(self):return self.action,self.owner,self.object,self.origin,self.group

@dataclass(frozen=True)
class Descriptor:
 body:bytes
 def __post_init__(self):
  p=self.body
  if not isinstance(p,bytes) or len(p)<HEADER or p[:8]!=MAGIC:raise Refus('version descripteur')
  count=p[295]
  if not 1<=count<=MAX_DEPENDENCIES or len(p)!=HEADER+count*DEPENDENCY:raise Refus('LIMIT : taille descripteur')
  for _,at,n in FIELDS:fixed(p[at:at+n],n)
  for name,at in NUMBERS:
   n=int.from_bytes(p[at:at+8],'big')
   if n>=2**63 or (name not in ('offset','object_revision','generation') and n==0):raise Refus('entier descripteur invalide')
  if int.from_bytes(p[292:294],'big') not in (62,183) or p[294] not in (1,2):raise Refus('architecture ou rôle invalide')
  if p[504]&~15 or p[505:512]!=bytes(7) or self.recipient!=self.subject:raise Refus('liaison destinataire invalide')
  if self.image_size>4*1024*1024 or self.offset+self.image_size>=2**63:raise Refus('LIMIT : image')
  ds=self.dependencies
  if sum(d.action==1 for d in ds)!=1 or ds[0].action!=1:raise Refus('installation nécessaire absente')
  if ds[0].owner!=self.subject or ds[0].object!=self.object:raise Refus('installation substituée')
  if [d.key() for d in ds]!=sorted({d.key() for d in ds}):raise Refus('dépendances dupliquées ou désordonnées')
  if any(max(d.membership_revision,d.rule_revision,d.image_revision)>self.generation for d in ds):raise Refus('provenance future')
 @property
 def dependencies(self):return tuple(Dependency.decode(self.body[i:i+DEPENDENCY]) for i in range(HEADER,len(self.body),DEPENDENCY))
 def __getattr__(self,name):
  for field,at,n in FIELDS:
   if field==name:return self.body[at:at+n]
  for field,at in NUMBERS:
   if field==name:return int.from_bytes(self.body[at:at+8],'big')
  raise AttributeError(name)
 @classmethod
 def encode(cls,values,dependencies):
  if not 1<=len(dependencies)<=MAX_DEPENDENCIES:raise Refus('LIMIT : dépendances')
  p=bytearray(HEADER);p[:8]=MAGIC
  for name,at,n in FIELDS:p[at:at+n]=fixed(values[name],n)
  for name,at in NUMBERS:
   v=values[name]
   if type(v) is not int or not 0<=v<2**63:raise Refus('entier invalide')
   p[at:at+8]=v.to_bytes(8,'big')
  version=values['version'];machine=values['machine'];role=values['role']
  if type(version) is not int or not 0<=version<2**32:raise Refus('version paquet invalide')
  if machine not in (62,183) or role not in (1,2):raise Refus('architecture ou rôle invalide')
  scope=values.get('administration',0)
  if type(scope) is not int or scope&~15:raise Refus('portée administrative invalide')
  p[504]=scope
  p[288:292]=version.to_bytes(4,'big');p[292:294]=machine.to_bytes(2,'big');p[294]=role;p[295]=len(dependencies)
  return cls(bytes(p)+b''.join(d.packet() for d in dependencies))
