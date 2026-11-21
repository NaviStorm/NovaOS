"""Publications invitées : authentification TLS, CAS durable et consultation seule."""
from cryptography.exceptions import InvalidSignature
from reprise import Recovery,Published,Refus,HEAD_CHANGE,fixed

def request(database,key,peer,scope,operation,body):
    server,domain=scope
    if len(body)<64 or body[:16]!=server or body[16:32]!=domain:
        raise Refus("portée substituée")
    instance=fixed(body[32:64],32)
    authority=Recovery(database,key)
    # Épinglage au pair enregistré avant mutation comme avant lecture d'issue.
    authority.observe_peer(server,domain,bytes([1])*32,peer)
    q=body[64:]
    if operation==30 and len(q)==224:
        state=Published(q[:16],q[16:32],q[32:64],
            int.from_bytes(q[64:72],"big"),int.from_bytes(q[72:80],"big"),q[80:112])
        if state.server!=server or state.domain!=domain or state.instance!=instance:
            raise Refus("route de mutation substituée")
        try:signature=authority.publish(state,q[112:128],q[128:160],q[160:224])
        except InvalidSignature as error:raise Refus("signature de mutation invalide") from error
        return signature
    if operation==31 and len(q)==48:
        packet,receipt,proof=authority.mutation_outcome(q[:16],q[16:])
        if not packet.startswith(HEAD_CHANGE) or packet[len(HEAD_CHANGE):len(HEAD_CHANGE)+32]!=server+domain:
            raise Refus("issue hors domaine")
        return packet+receipt+proof
    raise Refus("opération ou taille non autorisée")
