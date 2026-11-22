"""Route privée du registre via le signataire TLS, aucune commande administrative."""
from admission_exec import Admission, Admissions, FORMAT
from reprise import Recovery, Refus, fixed

ADMIT, FINISH, QUERY = 40, 41, 42


def request(database, key, peer, scope, operation, body, *, administration=False):
    server, domain = scope
    if len(body) < 64 or body[:32] != server+domain:
        raise Refus("portée substituée")
    incarnation = fixed(body[32:64],32)
    authority = Recovery(database,key)
    authority.observe_peer(server,domain,bytes([1])*32,peer)
    q = body[64:]
    if len(q) < FORMAT.size:
        raise Refus("requête tronquée")
    size = FORMAT.size
    if administration and q[:8] == b"NAEXAD01":
        from administration_epochs import SubjectAdmission
        size += 24
        admission = SubjectAdmission.decode(q[:size])
    else:
        admission = Admission.decode(q[:size])
    if (admission.server,admission.domain,admission.incarnation) != (server,domain,incarnation):
        raise Refus("incarnation substituée")
    api = Admissions(authority)
    q = q[size:]
    if operation == ADMIT and len(q) == 64:
        data,sig = api.admit(admission,q)
    elif operation == FINISH and len(q) == 97:
        data,sig = api.finish(admission,q[0],q[1:33],q[33:])
    elif operation == QUERY and len(q) == 96:
        data,sig = api.query(admission,q[:32],q[32:])
    else:
        raise Refus("opération ou taille non autorisée")
    return data+sig
