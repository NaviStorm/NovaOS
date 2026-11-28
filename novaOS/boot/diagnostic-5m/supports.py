"""Supports publics du laboratoire. Seules les trois empreintes d'image entrent dans une copie de db."""

import argparse
from hashlib import sha256
import json
from pathlib import Path
import struct
import sys
import uuid
from virt.firmware.varstore.edk2 import Edk2VarStore
from image import pe_info, check

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5e"))
from preparer import replace_once, volume

OWNER = "45434150-cdbe-4c63-8fcd-a45fa42834df"
SHA_GUID = uuid.UUID("c1c41626-504c-4092-aca9-41f936934328").bytes_le
MODES = {
    "actif": (
        "nominal",
        "e04b60150458454c7f9608209ac7358058c2bb566ddf254e455330c065b1411b",
    ),
    "inactif": (
        "inactif",
        "8c09b326b45951f539619b4d0f33e5e8fdbafa1cca9b173468f9df7e4a7725e0",
    ),
}


def digest(b):
    return sha256(b).hexdigest()


def sauver(p, d):
    p.write_text(json.dumps(d, indent=2, sort_keys=True) + "\n")


def sigs(raw):
    out = []
    pos = 0
    while pos < len(raw):
        guid = raw[pos : pos + 16]
        taille, head, one = struct.unpack_from("<III", raw, pos + 16)
        check(
            head == 0
            and one > 16
            and 28 < taille <= len(raw) - pos
            and (taille - 28) % one == 0,
            "EFI_SIGNATURE_LIST",
        )
        out.extend(
            (guid, raw[i : i + 16], raw[i + 16 : i + one])
            for i in range(pos + 28, pos + taille, one)
        )
        pos += taille
    return out


def comparer_magasin(avant, apres, ajouts):
    a = Edk2VarStore(str(avant)).get_varlist()
    b = Edk2VarStore(str(apres)).get_varlist()
    check(
        set(a) == set(b) and {"db", "dbx", "PK", "KEK"} <= set(a),
        "Variables différentes",
    )
    details = {}
    for nom in a:
        x, y = a[nom], b[nom]
        for attr in ("guid", "attr", "count", "pkidx", "time"):
            check(getattr(x, attr) == getattr(y, attr), "Métadonnée " + nom)
        if nom == "db":
            voulu = sigs(x.data) + [
                (SHA_GUID, uuid.UUID(OWNER).bytes_le, bytes.fromhex(h)) for h in ajouts
            ]
            check(
                sorted(voulu) == sorted(sigs(y.data)) and x.data != y.data,
                "Différence db non autorisée",
            )
        else:
            check(x.data == y.data and x.time == y.time, "Variable modifiée " + nom)
        details[nom] = dict(
            avant=digest(x.data),
            apres=digest(y.data),
            attributs=x.attr,
            horodatage_avant=str(x.time),
            horodatage_apres=str(y.time),
        )
    initial, final = avant.read_bytes(), apres.read_bytes()
    check(len(initial) == len(final), "Taille du magasin")
    return dict(
        original=digest(initial),
        copie=digest(final),
        variables=details,
        ajouts=ajouts,
        taille=len(final),
        octets_differents=sum(x != y for x, y in zip(initial, final)),
    )


def preparer(build5j, pe, out):
    out.mkdir(parents=True, exist_ok=False)
    raw = pe.read_bytes()
    refs = NOVA / "boot/diagnostic-5e/vecteurs"
    md = (refs / "ioapic.toml").read_bytes()
    manifeste = replace_once(
        raw, md, md.replace(b'class="diagnostic"', b'class="diagnostix"')
    )
    pkg = (NOVA / "libs/catalogue-pilotes/vecteurs/x86_64.pkg01").read_bytes()
    broken = bytearray(pkg)
    broken[4 + struct.unpack_from("<I", pkg)[0]] ^= 1
    invalide = replace_once(raw, pkg, bytes(broken))
    old = (build5j / "supports5j-b/diagnostic.efi").read_bytes()
    altered = (build5j / "supports5j-b/paquet.efi").read_bytes()
    check(
        digest(old)
        == "96a5a634b9a90f485b6bdfa0ac74f80a2d56d8819ff386d96c213b771f121239"
        and digest(altered)
        == "fbd459a90d10548b32d96406211f1f87b0d1ad56b8840a033efa4348bd950dbe",
        "Vecteurs publics 5J",
    )
    for nom in ("ioapic", "hpet-a", "hpet-b"):
        md = (refs / (nom + ".toml")).read_bytes()
        sig = (refs / (nom + ".sig")).read_bytes()
        i = old.index(sig)
        check(old.count(sig) == 1, "Signature historique ambiguë")
        invalide = replace_once(
            invalide, md, md.replace(digest(pkg).encode(), digest(broken).encode())
        )
        invalide = replace_once(invalide, sig, altered[i : i + 64])
    images = {}
    for nom, b in (("valide", raw), ("manifeste", manifeste), ("paquet", invalide)):
        (out / (nom + ".efi")).write_bytes(b)
        volume(out / (nom + ".img"), b)
        images[nom] = pe_info(b)
    sauver(out / "images.json", images)
    hashes = [v["authenticode"] for v in images.values()]
    check(len(set(hashes)) == 3, "Empreintes non distinctes")
    src = build5j / "qualification5j/nominal.fd"
    check(digest(src.read_bytes()) == MODES["actif"][1], "Origine du magasin actif")
    store = Edk2VarStore(str(src))
    vs = store.get_varlist()
    # Création hors ligne du seul fixture. Conserver aussi l’horodatage de db.
    for h in hashes:
        check(
            vs["db"].sigdb.add_hash(uuid.UUID(OWNER), bytes.fromhex(h)),
            "Empreinte déjà présente",
        )
    vs["db"].data = bytes(vs["db"].sigdb)
    cible = out / "actif.fd"
    store.write_varstore(str(cible), vs)
    sauver(out / "magasin.json", comparer_magasin(src, cible, hashes))
    check(digest(src.read_bytes()) == MODES["actif"][1], "Original modifié")
    other = build5j / "qualification5j/inactif.fd"
    check(digest(other.read_bytes()) == MODES["inactif"][1], "Magasin inactif")
    (out / "inactif.fd").write_bytes(other.read_bytes())


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    for x in ("build5j", "efi", "sortie"):
        p.add_argument(x, type=Path)
    a = p.parse_args()
    preparer(a.build5j, a.efi, a.sortie)
