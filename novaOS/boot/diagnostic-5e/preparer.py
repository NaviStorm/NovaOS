"""Fabrique uniquement des fichiers ordinaires neufs pour le laboratoire OVMF."""

import argparse
from datetime import datetime, timedelta, timezone
from hashlib import sha256
from pathlib import Path
import re
import struct
import subprocess
import tempfile
from uuid import UUID
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID
from pyfatfs.PyFat import PyFat
from pyfatfs.PyFatFS import PyFatFS

OWNER = str(UUID("45434150-cdbe-4c63-8fcd-a45fa42834df"))
BASE = Path(__file__).resolve().parent
NOVA = BASE.parents[1]


def identity(name, phase="5E"):
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    subject = x509.Name(
        [x509.NameAttribute(NameOID.COMMON_NAME, f"NovaOS {phase} laboratoire {name}")]
    )
    now = datetime.now(timezone.utc)
    cert = (
        x509.CertificateBuilder()
        .subject_name(subject)
        .issuer_name(subject)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - timedelta(days=1))
        .not_valid_after(now + timedelta(days=365))
        .add_extension(x509.BasicConstraints(ca=True, path_length=None), critical=True)
        .sign(key, hashes.SHA256())
    )
    return key, cert.public_bytes(serialization.Encoding.PEM)


def volume(path, pe):
    with path.open("xb") as stream:
        stream.truncate(8 * 1024 * 1024)
    fat = PyFat()
    fat.mkfs(
        str(path),
        fat_type=PyFat.FAT_TYPE_FAT16,
        size=path.stat().st_size,
        label="NOVA5ELAB",
        volume_id=0x053E0001,
    )
    fat.close()
    with PyFatFS(str(path)) as fs:
        fs.makedirs("EFI/BOOT")
        fs.writebytes("EFI/BOOT/BOOTX64.EFI", pe)
        assert fs.readbytes("EFI/BOOT/BOOTX64.EFI") == pe


def replace_once(data, old, new):
    if len(old) != len(new) or data.count(old) != 1:
        raise ValueError("Substitution de vecteur non univoque")
    return data.replace(old, new)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("unsigned", type=Path)
    p.add_argument("template", type=Path, help="gabarit OVMF vierge, lu uniquement")
    p.add_argument("out", type=Path, help="nouveau répertoire de fichiers de banc")
    p.add_argument("--signer", default="osslsigncode")
    p.add_argument("--var-tool", default="virt-fw-vars")
    p.add_argument("--phase5h", action="store_true", help="produire aussi le témoin pkg01 et le magasin OVMF inactif")
    a = p.parse_args()
    if not a.unsigned.is_file() or not a.template.is_file():
        raise ValueError("Le constructeur exige un PE et un gabarit ordinaires")
    out = a.out.resolve()
    out.mkdir(mode=0o700, parents=True, exist_ok=False)
    raw = a.unsigned.read_bytes()
    refs = BASE / "vecteurs"
    with tempfile.TemporaryDirectory(dir=out) as temporary:
        secret = Path(temporary)
        for name in ("PK", "KEK", "db"):
            key, cert = identity(name, "5H" if a.phase5h else "5E")
            (out / (name + ".pem")).write_bytes(cert)
            if name == "db":
                k = secret / "signer.pem"
                k.touch(mode=0o600)
                k.write_bytes(
                    key.private_bytes(
                        serialization.Encoding.PEM,
                        serialization.PrivateFormat.PKCS8,
                        serialization.NoEncryption(),
                    )
                )

        def signed(name, data):
            inp = secret / (name + ".bin")
            inp.write_bytes(data)
            output = out / (name + ".efi")
            subprocess.run(
                [
                    a.signer,
                    "sign",
                    "-h",
                    "sha256",
                    "-certs",
                    out / "db.pem",
                    "-key",
                    k,
                    "-in",
                    inp,
                    "-out",
                    output,
                ],
                check=True,
                stdout=subprocess.DEVNULL,
            )
            return output.read_bytes()

        trusted = signed("diagnostic", raw)
        volume(out / "diagnostic.img", trusted)
        config = b"NOVA5E-CFG!\x01\x05\x01\x00\x03"
        volume(
            out / "configuration.img",
            replace_once(trusted, config, config[:-2] + b"\x01\x03"),
        )
        old = signed(
            "ancienne", replace_once(raw, config, config[:12] + b"\x04" + config[13:])
        )
        volume(out / "ancienne.img", old)
        verified = subprocess.run(
            [
                a.signer,
                "verify",
                "-CAfile",
                out / "db.pem",
                "-in",
                out / "ancienne.efi",
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        old_hash = re.search(
            r"Current message digest\s*:\s*([0-9A-Fa-f]{64})", verified.stdout
        )
        if not old_hash:
            raise ValueError("Empreinte Authenticode de la génération précédente")
        pos = struct.unpack_from("<I", trusted, 0x3C)[0]
        count = struct.unpack_from("<H", trusted, pos + 6)[0]
        headers = pos + 24 + struct.unpack_from("<H", trusted, pos + 20)[0]
        code = next(
            i
            for i in range(headers, headers + count * 40, 40)
            if trusted[i : i + 8].rstrip(b"\0") == b".text"
        )
        offset = struct.unpack_from("<I", trusted, code + 20)[0] + 16
        changed = bytearray(trusted)
        changed[offset] ^= 1
        volume(out / "noyau.img", bytes(changed))
        # Mutation dans une section PE couverte par Authenticode, à longueur constante.
        manifest = (refs / "ioapic.toml").read_bytes()
        edited = manifest.replace(b'class="diagnostic"', b'class="diagnostix"')
        volume(out / "substitution.img", replace_once(trusted, manifest, edited))
        volume(out / "sans-signature.img", raw)
        volume(
            out / "signature-interne.img",
            signed("signature-interne", replace_once(raw, manifest, edited)),
        )
        # Un paquet correctement signé mais plus ancien que le plancher interne.
        from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
        from struct import pack

        older = manifest.replace(b"security_epoch=3", b"security_epoch=2")
        sig = Ed25519PrivateKey.from_private_bytes(bytes([1]) * 32).sign(
            b"NovaOS/driver-package/v1\0" + pack("<I", len(older)) + older
        )
        stale = replace_once(
            replace_once(raw, manifest, older), (refs / "ioapic.sig").read_bytes(), sig
        )
        volume(out / "epoque.img", signed("epoque", stale))
        if a.phase5h:
            # Modifier uniquement la signature ELF. Le manifeste pilote est re-signé
            # avec le bon digest du paquet modifié, pour atteindre le vérificateur interne.
            pkg = (NOVA / "libs/catalogue-pilotes/vecteurs/x86_64.pkg01").read_bytes()
            broken = bytearray(pkg)
            broken[4 + struct.unpack_from("<I", pkg)[0]] ^= 1
            patched = replace_once(raw, pkg, broken)
            signer = Ed25519PrivateKey.from_private_bytes(bytes([1]) * 32)
            for name in ("ioapic", "hpet-a", "hpet-b"):
                md = (refs / (name + ".toml")).read_bytes()
                new = replace_once(md, sha256(pkg).hexdigest().encode(), sha256(broken).hexdigest().encode())
                signature = signer.sign(b"NovaOS/driver-package/v1\0" + pack("<I", len(new)) + new)
                patched = replace_once(patched, md, new)
                patched = replace_once(patched, (refs / (name + ".sig")).read_bytes(), signature)
            volume(out / "paquet.img", signed("paquet", patched))
    # Initialisation hors ligne de nouveaux magasins. Le gabarit et OVMF restent en lecture seule.
    for name, extra in (
        ("nominal", []),
        ("revoque", ["--add-dbx-cert", OWNER, out / "db.pem"]),
    ):
        subprocess.run(
            [
                a.var_tool,
                "-i",
                a.template,
                "-o",
                out / (name + ".fd"),
                "--set-pk",
                OWNER,
                out / "PK.pem",
                "--add-kek",
                OWNER,
                out / "KEK.pem",
                "--add-db",
                OWNER,
                out / "db.pem",
                "--add-dbx-hash",
                OWNER,
                old_hash[1],
                "--secure-boot",
                *extra,
            ],
            check=True,
        )
    if a.phase5h:
        from virt.firmware.varstore.edk2 import Edk2VarStore
        base = Edk2VarStore(out / "nominal.fd")
        variables = base.get_varlist()
        variables.set_bool("SecureBootEnable", False)
        base.write_varstore(out / "inactif.fd", variables)
    print("Image signée", sha256((out / "diagnostic.img").read_bytes()).hexdigest())


if __name__ == "__main__":
    main()
