"""Route privée expérimentale, non activée par les bancs R2 historiques.

Le TLS et le signataire existants portent l'enveloppe authentifiée. La présence
d'un message ne prouve jamais un acquittement invité : le registre est tenu de
vérifier le canal et les faits de chaque effecteur avant de demander sa signature.
"""
from reprise import Recovery,Refus,fixed
from administration_mutation_rpc import state
from membership_retirement import Memberships,Source
RESERVE,PUBLISH,EFFECT,ADMIT,FINISH,QUERY,PENDING=range(54,61)

def request(database,key,peer,scope,operation,body,*,membership=False):
    if not membership: raise Refus('route sélective non activée')
    from reprise import transaction
    from membership_retirement import available
    with transaction(database) as db:
        if not available(db): raise Refus('registre sélectif non initialisé')
    if len(body)<128: raise Refus('taille invalide')
    server,domain=scope;incarnation=fixed(body[32:64],32)
    if body[:32]!=server+domain: raise Refus('portée substituée')
    q,proof=body[64:-64],body[-64:]
    lengths={PUBLISH:128,EFFECT:161,ADMIT:241,FINISH:194,QUERY:160,PENDING:144}
    size=225+33*q[224] if operation==RESERVE and len(q)>=225 and 1<=q[224]<=32 else lengths.get(operation)
    if size is None or len(q)!=size: raise Refus('opération ou taille invalide')
    current=state(q[:112])
    if (current.server,current.domain,current.instance)!=(server,domain,incarnation): raise Refus('incarnation substituée')
    authority=Recovery(database,key);authority.observe_peer(server,domain,bytes([1])*32,peer)
    api=Memberships(authority)
    if operation==RESERVE:
        expected=tuple((q[i],q[i+1:i+33]) for i in range(225,len(q),33))
        args=(current,q[112:128],q[128:160],q[160:176],int.from_bytes(q[176:184],'big'),Source(q[184:200],q[200:216],int.from_bytes(q[216:224],'big')),expected)
        if api.reservation(*args)!=q: raise Refus('réservation non canonique')
        api.reserve(*args,proof)
    elif operation==PUBLISH:
        return api.publish(current,q[112:128],proof)
    elif operation==EFFECT:
        api.acknowledge(current,q[112:128],q[128],q[129:161],proof)
    elif operation==ADMIT:
        source=Source(q[112:128],q[128:144],int.from_bytes(q[144:152],'big'))
        api.admit(current,source,q[152],q[153:185],q[185:201],int.from_bytes(q[201:209],'big'),q[209:241],proof)
    elif operation==FINISH:
        api.finish(current,q[112:128],q[128],q[129:161],q[161],q[162:194],proof)
        # Progresser uniquement après le fait terminal durable et authentifié,
        # comme le raccord EXEC historique ; QUERY demeure une lecture pure.
        from administration_mutation_rpc import progress
        progress(api.epochs,server,domain)
    elif operation==QUERY:
        from reprise import transaction
        with transaction(database) as db:
            launch=db.execute('SELECT 1 FROM membership_operations WHERE op=? AND server=? AND domain=?',(q[112:128],server,domain)).fetchone()
        method=api.operation_outcome if launch else api.outcome
        reply,signature=method(current,q[112:128],q[128:160],proof)
        return reply+signature
    else:
        reply,signature=api.pending(current,q[112:],proof)
        return reply+signature
    # Le succès de transport n'est pas un reçu. Le client doit interroger l'issue
    # avec un défi frais, y compris après perte de ce simple accusé de traitement.
    return b''
