"""R2 privé : publication et acquittements du registre isolé, jamais du CLI.

Les identités/capacités administratives sont vérifiées par le registre. Cette
route vérifie son serveur authentifié et conserve l'ordre durable existant.
"""
import struct
from reprise import Recovery, Refus, Published, fixed
from administration_epochs import Epochs

PUBLISH, ACKNOWLEDGE, OUTCOME = 50, 51, 52


def state(raw):
    if len(raw) != 112:
        raise Refus("taille de tête administrative invalide")
    value = Published(*struct.unpack('>16s16s32sQQ32s', raw))
    if value.packet() != raw:
        raise Refus("tête non canonique")
    return value


def progress(api, server, domain):
    """Transition interne après un fait durable ; jamais lors d'une consultation.

    Les acquittements manquants ou admissions non terminales restent en cours.
    Aucun délai, retour de transport ou refus n'est converti en annulation.
    """
    from reprise import transaction
    with transaction(api.path) as db:
        pending = db.execute(
            "SELECT request FROM subject_epochs WHERE server=? AND domain=? AND state='RETRAIT_EN_COURS'",
            (server, domain)).fetchall()
    for (operation,) in pending:
        try:
            api.drain(server, domain, operation)
        except Refus as error:
            if not str(error).startswith('UNCERTAIN :'):
                # Une autre transition peut avoir terminé le même retrait.
                with transaction(api.path) as db:
                    terminal = db.execute(
                        'SELECT terminal FROM admin_changes WHERE op=? AND server=? AND domain=?',
                        (operation, server, domain)).fetchone()
                if terminal is None or terminal[0] is None:
                    raise


def request(database, key, peer, scope, operation, body, *, administration=False):
    if not administration:
        raise Refus("route administrative R2 non activée")
    sizes = {PUBLISH: 209, ACKNOWLEDGE: 129, OUTCOME: 160}
    size = sizes.get(operation)
    if size is None or len(body) != 64 + size + 64:
        raise Refus("opération ou taille administrative invalide")
    server, domain = scope
    incarnation = fixed(body[32:64], 32)
    if body[:32] != server + domain:
        raise Refus("portée substituée")
    data, proof = body[64:-64], body[-64:]
    current = state(data[:112])
    if (current.server, current.domain, current.instance) != (server, domain, incarnation):
        raise Refus("incarnation substituée")
    authority = Recovery(database, key)
    authority.observe_peer(server, domain, bytes([1])*32, peer)
    api = Epochs(authority)
    if operation == PUBLISH:
        args = (current, data[112:128], data[128:160], data[160:176],
                int.from_bytes(data[176:184], 'big'), data[184:200],
                int.from_bytes(data[200:208], 'big'), data[208])
        if api.change(*args) != data:
            raise Refus("publication non canonique")
        return api.publish(*args, proof)
    if operation == ACKNOWLEDGE:
        result, signature = api.acknowledge(current, data[112:128], data[128], proof)
        progress(api, server, domain)
    else:
        # Lecture pure. Ne fait pas progresser le retrait et ne répète rien.
        result, signature = api.outcome(current, data[112:128], data[128:160], proof)
    return result + signature
