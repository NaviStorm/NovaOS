"""Recette du transport. Les deux extrémités cryptographiques sont des processus hôtes isolés du relais."""
from contextlib import ExitStack
import datetime
import hashlib
import multiprocessing
import os
from pathlib import Path
import ssl
import struct
import tempfile

from cryptography import x509
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID
from adaptateur import lire

PEM = serialization.Encoding.PEM
DER = serialization.Encoding.DER
REQUEST = b"NOVAAUTH/fixture/decision/1\0"
RECEIPT = b"NOVAAUTH/fixture/receipt/1\0"
QUERY = b"NOVAAUTH/fixture/outcome/1\0"


def certificate(ca_key, name, public):
    now = datetime.datetime.now(datetime.timezone.utc)
    ca = name == "ca-test"
    def subject(s):
        return x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, s)])
    cert = (x509.CertificateBuilder().subject_name(subject(name)).issuer_name(subject("ca-test"))
            .public_key(public).serial_number(x509.random_serial_number())
            .not_valid_before(now - datetime.timedelta(minutes=1))
            .not_valid_after(now + datetime.timedelta(hours=1))
            .add_extension(x509.BasicConstraints(ca, 0 if ca else None), critical=True)
            .add_extension(x509.KeyUsage(True, False, False, False, False, ca, ca, False, False), critical=True)
            .add_extension(x509.SubjectKeyIdentifier.from_public_key(public), critical=False)
            .add_extension(x509.AuthorityKeyIdentifier.from_issuer_public_key(ca_key.public_key()), critical=False))
    if not ca:
        usage = ExtendedKeyUsageOID.SERVER_AUTH if name == "autorite" else ExtendedKeyUsageOID.CLIENT_AUTH
        cert = (cert.add_extension(x509.ExtendedKeyUsage([usage]), critical=False)
                .add_extension(x509.SubjectAlternativeName([x509.DNSName(name)]), critical=False))
    return cert.sign(ca_key, hashes.SHA256())


def endpoint(control, server):
    # spawn crée cet espace de processus avant la génération de ses clés.
    tls_key = ec.generate_private_key(ec.SECP256R1())
    signing_key = ed25519.Ed25519PrivateKey.generate()
    control.send((tls_key.public_key().public_bytes(DER, serialization.PublicFormat.SubjectPublicKeyInfo),
                  signing_key.public_key().public_bytes_raw()))
    ca, cert, pinned, peer_key, context, op = control.recv()
    public = ed25519.Ed25519PublicKey.from_public_bytes(peer_key)
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER if server else ssl.PROTOCOL_TLS_CLIENT)
    ctx.minimum_version = ctx.maximum_version = ssl.TLSVersion.TLSv1_3
    ctx.verify_mode = ssl.CERT_REQUIRED
    ctx.load_verify_locations(cadata=ca)
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as d:
        path = Path(d) / "identite-ephemere.pem"
        with open(path, "xb", opener=lambda p, f: os.open(p, f, 0o600)) as f:
            f.write(cert)
            f.write(tls_key.private_bytes(PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
        ctx.load_cert_chain(path)
    if server:
        ctx.num_tickets = 0
    nonce = os.urandom(32)
    receipt = None
    incoming = outgoing = tls = None
    control.send(nonce if server else None)
    try:
        while True:
            action, data = control.recv()
            if action == "fin":
                return
            if action == "neuf":
                incoming, outgoing = ssl.MemoryBIO(), ssl.MemoryBIO()
                tls = ctx.wrap_bio(incoming, outgoing, server_side=server, server_hostname=None if server else "autorite")
                result = None
            elif action == "negocier":
                if data:
                    incoming.write(data)
                try:
                    tls.do_handshake()
                    if tls.version() != "TLSv1.3" or tls.session_reused or tls.getpeercert(True) != pinned:
                        raise ValueError("pair TLS ou reprise implicite")
                    done = True
                except ssl.SSLWantReadError:
                    done = False
                result = done, outgoing.read()
            elif action == "emettre":
                if tls.write(data) != len(data):
                    raise ValueError("écriture TLS partielle")
                result = outgoing.read()
            elif action == "lire":
                incoming.write(data)
                try:
                    result = tls.read(2048)
                except ssl.SSLError as err:
                    if err.reason not in {"RECORD_LAYER_FAILURE", "DECRYPTION_FAILED_OR_BAD_RECORD_MAC"}:
                        raise
                    result = None  # Erreur fatale de couche enregistrement.
            elif action == "signer" and not server and len(data) == 32:
                fields = REQUEST + context + op + data
                result = fields + signing_key.sign(fields)
            elif action == "admettre" and server:
                fields = REQUEST + context + op + nonce
                try:
                    public.verify(data[-64:], data[:-64])
                    valid = data[:-64] == fields
                except InvalidSignature:
                    valid = False
                if not valid or receipt is not None:
                    result = False
                else:
                    body = RECEIPT + op + hashlib.sha256(fields).digest()
                    receipt = body + signing_key.sign(body)
                    result = True
            elif action == "issue" and server and receipt is not None:
                if len(data)!=len(QUERY)+16+32 or not data.startswith(QUERY+op):
                    raise ValueError("consultation inconnue")
                result = receipt + signing_key.sign(data + receipt)
            elif action == "verifier" and not server:
                query, answer, fields = data
                public.verify(answer[-64:], query + answer[:-64])
                public.verify(answer[-128:-64], answer[:-128])
                result = answer[:-128] == RECEIPT + op + hashlib.sha256(fields).digest()
            else:
                raise ValueError("commande de recette invalide")
            control.send(result)
    finally:
        control.close()


def qualify(sock):
    vols = 0
    def travel(data):
        nonlocal vols
        sock.sendall(data)
        echo = bytes(lire(sock, len(data)))
        if echo != data:
            raise ValueError("octets altérés sur le trajet invité")
        vols += 1
        return echo

    for size in (1, 448, 449, 2048):
        travel(bytes(i % 256 for i in range(size)))

    def finish(proc, conn):
        try:
            if proc.is_alive():
                conn.send(("fin", None))
        except OSError:
            pass
        finally:
            conn.close()
            proc.join(3)
            if proc.is_alive():
                proc.terminate()
                proc.join(3)
            proc.close()

    ctx = multiprocessing.get_context("spawn")
    ca_key = ec.generate_private_key(ec.SECP256R1())
    ca = certificate(ca_key, "ca-test", ca_key.public_key()).public_bytes(PEM).decode()
    def recv(conn):
        if not conn.poll(10):
            raise TimeoutError("extrémité cryptographique")
        return conn.recv()

    with ExitStack() as stack:
        controls, public, certs, pids = [], [], [], []
        for i, name in enumerate(("signataire", "autorite")):
            local, remote = ctx.Pipe()
            proc = ctx.Process(target=endpoint, args=(remote, bool(i)))
            proc.start()
            remote.close()
            stack.callback(finish, proc, local)
            key, identity = recv(local)
            public.append(identity)
            certs.append(certificate(ca_key, name, serialization.load_der_public_key(key)))
            controls.append(local)
            pids.append(proc.pid)
        # Serveur, domaine, session, instance de registre, génération, empreinte.
        context = os.urandom(16*3+32) + struct.pack(">Q", (1<<64)-1) + os.urandom(32)
        op = os.urandom(16)
        for i, conn in enumerate(controls):
            conn.send((ca, certs[i].public_bytes(PEM), certs[1-i].public_bytes(DER), public[1-i], context, op))
        recv(controls[0])
        nonce = recv(controls[1])
        def rpc(i, name, data=None):
            controls[i].send((name, data))
            return recv(controls[i])
        def connect():
            pending = [b"", b""]
            ready = [False, False]
            for i in (0,1):
                rpc(i,"neuf")
            for _ in range(12):
                for i in (0,1):
                    if not ready[i]:
                        ready[i], flight = rpc(i,"negocier",pending[i])
                        pending[i] = b""
                        if flight:
                            pending[1-i] += travel(flight)
                if all(ready) and not any(pending):
                    return
            raise ValueError("échec d’authentification mutuelle")
        def deliver(i, raw):
            return rpc(1-i,"lire",travel(raw))
        def message(i, content):
            data = deliver(i,rpc(i,"emettre",content))
            if data != content:
                raise ValueError("réception TLS inexacte")
            return data

        connect()
        signed = rpc(0,"signer",message(1,nonce))
        bad = bytearray(signed)
        bad[-65] ^= 1
        if rpc(1,"admettre",message(0,bytes(bad))) or not rpc(1,"admettre",message(0,signed)):
            raise ValueError("admission ou contrôle de signature")
        query = QUERY + op + os.urandom(32)
        answer = rpc(1,"issue",message(0,query))
        travel(rpc(1,"emettre",answer))  # Réponse jetée avant remise au client.
        connect()
        if rpc(1,"admettre",message(0,signed)):
            raise ValueError("rejeu applicatif admis")
        query = QUERY + op + os.urandom(32)
        reread = message(1,rpc(1,"issue",message(0,query)))
        if reread[:-64]!=answer[:-64] or not rpc(0,"verifier",(query,reread,signed[:-64])):
            raise ValueError("issue historique ou fraîcheur incorrecte")
        encrypted = rpc(0,"emettre",query)
        if deliver(0,encrypted)!=query or deliver(0,encrypted) is not None:
            raise ValueError("rejeu TLS accepté")
        connect()
        damaged = bytearray(rpc(0,"emettre",query))
        damaged[-1] ^= 1
        if deliver(0,bytes(damaged)) is not None:
            raise ValueError("enregistrement altéré accepté")
    return dict(vols=vols, tls="1.3 mutuel", pid_signataire_hote=pids[0],pid_autorite_hote=pids[1],
                octets=[1,448,449,2048],taille_signature=64,empreinte_recu=hashlib.sha256(reread[:-64]).hexdigest(),
                signature_invalide_refusee=True,rejeu_applicatif_refuse=True,rejeu_tls_refuse=True,
                alteration_tls_refusee=True,relecture_sur_connexion_neuve=True)
