"""Extension privée opt-in du canal registre/signataire, sans repli historique."""
from reprise import Recovery,Refus,fixed,transaction
from administration_epochs import SubjectAdmission
from exec_descriptor import Descriptor
from exec_selective import Selective,enabled
REGISTER,LOOKUP,ADMIT,CANCEL=62,63,64,65

def request(database,key,peer,scope,operation,body,*,selective=False):
 if not selective:raise Refus('revalidation sélective non activée')
 if len(body)<128:raise Refus('requête tronquée')
 server,domain=scope;incarnation=fixed(body[32:64],32)
 if body[:32]!=server+domain:raise Refus('portée substituée')
 with transaction(database) as db:
  if not enabled(db):raise Refus('registre descripteurs non initialisé')
 authority=Recovery(database,key);authority.observe_peer(server,domain,bytes([1])*32,peer)
 api=Selective(authority);q,proof=body[64:-64],body[-64:]
 if operation==REGISTER:
  d=Descriptor(q)
  if (d.server,d.domain,d.incarnation)!=(server,domain,incarnation):raise Refus('incarnation substituée')
  result=api.register(d,proof)
 elif operation==LOOKUP and len(q)==112:
  if q[:64]!=body[:64]:raise Refus('portée historique substituée')
  result=api.lookup(server,domain,incarnation,q[64:80],q[80:],proof)
 elif operation in (ADMIT,CANCEL) and len(q)==312:
  admission=SubjectAdmission.decode(q[:280])
  if (admission.server,admission.domain,admission.incarnation)!=(server,domain,incarnation):raise Refus('incarnation substituée')
  result=(api.admit if operation==ADMIT else api.cancel)(admission,q[280:],proof)
 else:raise Refus('opération ou taille non autorisée')
 return result[0]+result[1]
