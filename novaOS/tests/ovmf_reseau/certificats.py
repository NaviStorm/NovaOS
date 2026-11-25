"""Prépare deux supports Secure Boot privés à partir des EFI synthétiques du banc."""
import argparse
import hashlib
from pathlib import Path
import os
import subprocess
import tempfile

import pefile
from virt.firmware.varstore import autodetect
from virt.peutils.pesign import pe_authenticode_hash

OWNER = "7a58904e-20f7-47ae-9f7c-674f87ef1104"


def signer(src, dest, cert, key):
    subprocess.run(["osslsigncode", "sign", "-certs", cert, "-key", key, "-h", "sha256",
                    "-in", src, "-out", dest], check=True)
    subprocess.run(["osslsigncode", "verify", "-CAfile", cert, "-in", dest], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("temoin", type=Path)
    parser.add_argument("modele_vars", type=Path)
    parser.add_argument("sortie", type=Path)
    parser.add_argument("--cle-distincte", action="store_true", help="Révoquer le certificat du second PE dans dbx")
    a = parser.parse_args()
    out = a.sortie.resolve()
    out.mkdir()
    source = a.temoin / "esp/EFI/BOOT"
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"], prefix="ovmf-cert-") as td:
        tmp = Path(td)
        key = tmp / "test.key"
        cert = out / "test.crt"
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-noenc", "-days", "2",
                        "-subj", "/CN=NovaOS Q35 essai EFI/", "-addext", "extendedKeyUsage=codeSigning",
                        "-keyout", key, "-out", cert], check=True, stderr=subprocess.DEVNULL)
        autorise = tmp / "autorise.efi"
        revoque = tmp / "revoque.efi"
        signer(source / "BOOTX64.EFI", autorise, cert, key)
        second_cert = cert
        second_key = key
        if a.cle_distincte:
            second_cert = out / "signataire-second.crt"
            second_key = tmp / "second.key"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-noenc", "-days", "2",
                            "-subj", "/CN=NovaOS Q35 signataire secondaire/", "-addext", "extendedKeyUsage=codeSigning",
                            "-keyout", second_key, "-out", second_cert], check=True, stderr=subprocess.DEVNULL)
        signer(source / "REFUSE.EFI", revoque, second_cert, second_key)
        store = autodetect.open_varstore(str(a.modele_vars))
        variables = store.get_varlist()
        variables.enroll_platform_generate("NovaOS Q35 certificat laboratoire", ms_kek="none")
        variables.add_cert("db", OWNER, str(cert))
        if a.cle_distincte:
            variables.add_cert("db", OWNER, str(second_cert))
        variables.enable_secureboot()
        for cas in ("signature-alteree", "avant-dbx", "dbx"):
            dest = out / cas
            esp = dest / "esp/EFI/BOOT"
            esp.mkdir(parents=True)
            bon = autorise.read_bytes()
            (esp / "BOOTX64.EFI").write_bytes(bon)
            if cas != "signature-alteree":
                if cas == "dbx":
                    if a.cle_distincte:
                        variables.add_cert("dbx", OWNER, str(second_cert))
                    else:
                        variables.add_hash("dbx", OWNER, pe_authenticode_hash(pefile.PE(str(revoque))).hex())
                mauvais = revoque.read_bytes()
            else:
                # Un octet de texte dans une section couverte. Conserver PE/COFF et le certificat.
                if bon.count(b"NET EFI controle fini") != 1:
                    raise ValueError("Emplacement de la sentinelle ambigu")
                mauvais = bon.replace(b"NET EFI controle fini", b"NET EFI controle finX")
                if pe_authenticode_hash(pefile.PE(data=mauvais)) == pe_authenticode_hash(pefile.PE(data=bon)):
                    raise ValueError("Le caractère altéré doit être couvert par Authenticode")
            (esp / "REFUSE.EFI").write_bytes(mauvais)
            store.write_varstore(str(dest / "variables.fd"), variables)
            if cas == "dbx":
                # Les deux PE sont identiques. Réutiliser le disque bit pour bit,
                # même si la construction hôte traverse un changement de date FAT.
                os.link(out / "avant-dbx/support.raw", dest / "support.raw")
            else:
                subprocess.run(["qemu-img", "convert", "-f", "raw", "-O", "raw",
                                f"fat:{dest}/esp", dest / "support.raw"], check=True)
            print(cas, "SHA-256 EFI admise", hashlib.sha256(bon).hexdigest(),
                  "candidate refusée", hashlib.sha256(mauvais).hexdigest())


if __name__ == "__main__":
    main()
