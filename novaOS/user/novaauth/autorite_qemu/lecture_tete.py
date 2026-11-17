"""Consultation privée de la tête courante après authentification mutuelle TLS."""
from cryptography import x509
from reprise import Recovery, Refus


def respond(connection, request, database, signing_key):
    if len(request) != 104 or request[:8] != b"NACURR01":
        raise Refus("format de consultation")
    server, domain = request[8:24], request[24:40]
    instance, fresh = request[40:72], request[72:]
    if not any(instance) or not any(fresh):
        raise Refus("instance ou défi vide")
    cert = x509.load_der_x509_certificate(connection.getpeercert(binary_form=True))
    key = cert.public_key().public_bytes_raw()
    authority = Recovery(database, signing_key)
    state, signature = authority.observe_peer(server, domain, fresh, key)
    return state.packet() + signature
