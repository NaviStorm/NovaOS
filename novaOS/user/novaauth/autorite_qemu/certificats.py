"""Prépare les certificats éphémères de qualification depuis la clé publique du signataire."""
import argparse
from datetime import datetime, timedelta, timezone
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey, Ed25519PublicKey
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID


def issue(guest_public, directory):
    guest = Ed25519PublicKey.from_public_bytes(guest_public.read_bytes())
    directory.mkdir(mode=0o700, exist_ok=False)
    ca = Ed25519PrivateKey.generate()
    authority = Ed25519PrivateKey.generate()
    now = datetime.now(timezone.utc)

    def name(value):
        return x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, value)])

    for label, public in (("ca", ca.public_key()), ("autorite", authority.public_key()), ("serveur", guest)):
        cert = (x509.CertificateBuilder().subject_name(name(label)).issuer_name(name("ca"))
                .public_key(public).serial_number(x509.random_serial_number())
                .not_valid_before(now - timedelta(minutes=1)).not_valid_after(now + timedelta(days=1))
                .add_extension(x509.BasicConstraints(ca=label == "ca", path_length=None), critical=True))
        if label != "ca":
            usage = ExtendedKeyUsageOID.SERVER_AUTH if label == "autorite" else ExtendedKeyUsageOID.CLIENT_AUTH
            cert = cert.add_extension(x509.ExtendedKeyUsage([usage]), critical=False)
        signed = cert.sign(ca, None)
        for suffix, encoding in (("der", serialization.Encoding.DER), ("pem", serialization.Encoding.PEM)):
            (directory / f"{label}.{suffix}").write_bytes(signed.public_bytes(encoding))
    secret = directory / "autorite-cle.pem"
    secret.touch(mode=0o600, exist_ok=False)
    secret.write_bytes(authority.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8,
                                              serialization.NoEncryption()))
    (directory / "autorite.pub").write_bytes(authority.public_key().public_bytes_raw())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cle_publique_invitee", type=Path)
    parser.add_argument("nouveau_repertoire", type=Path)
    args = parser.parse_args()
    issue(args.cle_publique_invitee, args.nouveau_repertoire)
