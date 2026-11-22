import tempfile, unittest
from pathlib import Path
from datetime import timedelta
from cryptography import x509
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from certificats import issue, validity

class Validity(unittest.TestCase):
    def test_expiry_and_margin_do_not_disable_tls_checks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);public=root/'public'
            public.write_bytes(Ed25519PrivateKey.generate().public_key().public_bytes_raw())
            directory=root/'certs';issue(public,directory)
            c=x509.load_der_x509_certificate((directory/'serveur.der').read_bytes())
            self.assertEqual(len(validity(directory,c.not_valid_before_utc+timedelta(minutes=2))),3)
            for now in (c.not_valid_before_utc-timedelta(seconds=1),c.not_valid_after_utc,c.not_valid_after_utc-timedelta(seconds=60)):
                with self.assertRaises(ValueError):validity(directory,now)
            with self.assertRaises(ValueError):validity(directory,remaining_seconds=-1)

if __name__=='__main__':unittest.main()
